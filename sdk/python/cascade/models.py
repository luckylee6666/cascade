from dataclasses import dataclass
from typing import Optional
from datetime import datetime


@dataclass
class Config:
    id: str
    key: str
    value: Optional[str]
    secret: bool
    group: Optional[str]
    description: Optional[str]
    created_at: Optional[datetime] = None
    updated_at: Optional[datetime] = None


@dataclass
class Project:
    id: str
    name: str
    description: Optional[str]
    created_at: Optional[datetime] = None


@dataclass
class Environment:
    id: str
    name: str
    parent_id: Optional[str]
    created_at: Optional[datetime] = None
