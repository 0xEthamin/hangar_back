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