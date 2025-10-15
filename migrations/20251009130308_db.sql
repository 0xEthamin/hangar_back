CREATE TYPE project_source_type AS ENUM ('direct', 'github');

CREATE TABLE projects
(
    id SERIAL PRIMARY KEY,

    name VARCHAR(63) NOT NULL UNIQUE,

    owner VARCHAR(255) NOT NULL,

    container_name VARCHAR(255) NOT NULL UNIQUE,

    source_type project_source_type NOT NULL,
    
    source_url VARCHAR(2048) NOT NULL,

    deployed_image_tag VARCHAR(2048) NOT NULL,

    env_vars JSONB NULL,

    persistent_volume_path VARCHAR(2048) NULL,

    volume_name VARCHAR(255) NULL,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_projects_owner ON projects(owner);

CREATE TABLE project_participants
(
    project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,

    participant_id VARCHAR(10) NOT NULL,

    PRIMARY KEY (project_id, participant_id)
);

CREATE INDEX idx_project_participants_participant_id ON project_participants(participant_id);


CREATE TABLE databases
(
    id SERIAL PRIMARY KEY,

    owner_login VARCHAR(255) NOT NULL UNIQUE,

    database_name VARCHAR(255) NOT NULL UNIQUE,

    username VARCHAR(255) NOT NULL UNIQUE,

    encrypted_password TEXT NOT NULL,

    project_id INTEGER NULL REFERENCES projects(id) ON DELETE SET NULL,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_databases_owner_login ON databases(owner_login);