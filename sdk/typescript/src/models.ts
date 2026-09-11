export interface Config {
  id: string;
  key: string;
  value: string | null;
  secret: boolean;
  source?: string;
  group: string | null;
  description: string | null;
}

export interface Project {
  id: string;
  name: string;
  description: string | null;
}

export interface Environment {
  id: string;
  name: string;
  parent_id: string | null;
}

/** Receives the raw SSE event, e.g. "config_updated:<id>". */
export type ChangeCallback = (event: string) => void;
