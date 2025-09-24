CREATE TABLE projects
(
    id SERIAL PRIMARY KEY,

    -- Match la validation de validation_service.rs
    name VARCHAR(63) NOT NULL UNIQUE,

    owner VARCHAR(255) NOT NULL,

    image_url VARCHAR(2048) NOT NULL,
    
    container_id VARCHAR(255) NOT NULL,

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