pub mod proto {
    tonic::include_proto!("configcenter");
}

use proto::config_center_server::ConfigCenter;
use std::{pin::Pin, sync::Arc};
use tokio_stream::{wrappers::BroadcastStream, Stream, StreamExt};
use tonic::{Request, Response, Status};

use crate::AppState;

pub struct GrpcService {
    state: Arc<AppState>,
}

impl GrpcService {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

type EventStream =
    Pin<Box<dyn Stream<Item = Result<proto::Config, Status>> + Send>>;
type WatchStream =
    Pin<Box<dyn Stream<Item = Result<proto::ConfigEvent, Status>> + Send>>;

fn to_proto(c: &cc_core::config::Config) -> proto::Config {
    proto::Config {
        id: c.id.clone(),
        key: c.key.clone(),
        // Secrets never leave the server unmasked (see proto docs).
        value: if c.secret {
            "***".to_string()
        } else {
            c.value.clone().unwrap_or_default()
        },
        secret: c.secret,
        group: c.group.clone().unwrap_or_default(),
        description: c.description.clone().unwrap_or_default(),
        created_at: c.created_at.timestamp(),
        updated_at: c.updated_at.timestamp(),
    }
}

fn to_proto_resolved(c: &cc_core::config::ConfigWithValue) -> proto::Config {
    let mut p = to_proto(&c.config);
    if !c.config.secret {
        p.value = c.effective_value.clone();
    }
    p
}

fn db_err(e: impl std::fmt::Display) -> Status {
    tracing::warn!(error = %e, "grpc db error");
    Status::internal("database error")
}

#[tonic::async_trait]
impl ConfigCenter for GrpcService {
    type ListConfigsStream = EventStream;
    type WatchConfigsStream = WatchStream;

    async fn get_config(
        &self,
        req: Request<proto::GetConfigRequest>,
    ) -> Result<Response<proto::Config>, Status> {
        let repo = cc_store::config_repo::ConfigRepo::new(&self.state.store);
        let cfg = repo
            .get_by_key(&req.into_inner().key)
            .map_err(db_err)?
            .ok_or_else(|| Status::not_found("config not found"))?;
        Ok(Response::new(to_proto(&cfg)))
    }

    async fn list_configs(
        &self,
        req: Request<proto::ListConfigsRequest>,
    ) -> Result<Response<EventStream>, Status> {
        let q = req.into_inner();
        let store = &self.state.store;
        let items: Vec<proto::Config> = if !q.project_id.is_empty() && !q.env_id.is_empty() {
            // Resolved project snapshot: attachments-only, effective values,
            // secrets masked.
            let scope =
                cc_store::resolve::resolve_project_scope(store, &q.project_id, Some(&q.env_id))
                    .map_err(|e| {
                        let msg = e.to_string();
                        if msg.contains("not found") || msg.contains("No environments") {
                            Status::not_found(msg)
                        } else {
                            Status::failed_precondition(msg)
                        }
                    })?;
            scope
                .configs
                .iter()
                .filter(|c| q.group.is_empty() || c.config.group.as_deref() == Some(q.group.as_str()))
                .map(to_proto_resolved)
                .collect()
        } else {
            let repo = cc_store::config_repo::ConfigRepo::new(store);
            repo.list(if q.group.is_empty() {
                None
            } else {
                Some(q.group.as_str())
            })
            .map_err(db_err)?
            .iter()
            .map(to_proto)
            .collect()
        };
        let stream = tokio_stream::iter(items.into_iter().map(Ok));
        Ok(Response::new(Box::pin(stream)))
    }

    async fn watch_configs(
        &self,
        req: Request<proto::WatchRequest>,
    ) -> Result<Response<WatchStream>, Status> {
        let q = req.into_inner();
        let filter: Option<std::collections::HashSet<String>> = if q.keys.is_empty() {
            None
        } else {
            Some(q.keys.into_iter().collect())
        };
        let state = self.state.clone();
        let rx = state.tx.subscribe();
        // NOTE: tokio-stream's filter_map takes a SYNC closure (unlike
        // futures'). No await needed here anyway — rusqlite calls block.
        let stream = BroadcastStream::new(rx).filter_map(move |msg| {
            let text = msg.ok()?;
            // Broadcast payloads look like "config_updated:<id>".
            let (kind, id) = match text.split_once(':') {
                Some((k, id)) => (k.to_string(), Some(id.to_string())),
                None => (text, None),
            };
            let config = id.and_then(|id| {
                cc_store::config_repo::ConfigRepo::new(&state.store)
                    .get(&id)
                    .ok()
                    .flatten()
                    .map(|c| to_proto(&c))
            });
            if let (Some(keys), Some(cfg)) = (&filter, &config) {
                if !keys.contains(&cfg.key) {
                    return None;
                }
            }
            Some(Ok(proto::ConfigEvent {
                event_type: kind,
                config,
            }))
        });
        Ok(Response::new(Box::pin(stream)))
    }

    async fn get_project_configs(
        &self,
        req: Request<proto::GetProjectRequest>,
    ) -> Result<Response<proto::ProjectConfigSnapshot>, Status> {
        let q = req.into_inner();
        let store = &self.state.store;
        let scope =
            cc_store::resolve::resolve_project_scope(store, &q.project_id, Some(&q.env_id))
                .map_err(|e| {
                    let msg = e.to_string();
                    if msg.contains("not found") || msg.contains("No environments") {
                        Status::not_found(msg)
                    } else {
                        Status::failed_precondition(msg)
                    }
                })?;
        Ok(Response::new(proto::ProjectConfigSnapshot {
            project_id: scope.project.id,
            project_name: scope.project.name,
            env_id: scope.env.id,
            env_name: scope.env.name,
            configs: scope.configs.iter().map(to_proto_resolved).collect(),
        }))
    }
}

/// Bearer-token interceptor with the same semantics as the HTTP middleware.
/// No token configured → open (loopback-only deployments, enforced at boot).
pub fn auth_interceptor(
    state: Arc<AppState>,
) -> impl FnMut(tonic::Request<()>) -> Result<tonic::Request<()>, Status> + Clone {
    move |req: tonic::Request<()>| {
        if state.auth_token.is_none() {
            return Ok(req);
        }
        let ok = req
            .metadata()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .map(|t| {
                cc_store::token_repo::TokenRepo::new(&state.store)
                    .check(t.trim())
                    .valid
            })
            .unwrap_or(false);
        if ok {
            Ok(req)
        } else {
            Err(Status::unauthenticated("bad or missing token"))
        }
    }
}
