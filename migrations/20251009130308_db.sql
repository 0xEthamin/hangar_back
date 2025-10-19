-- Définit un type de données personnalisé pour spécifier l'origine du projet.
-- 'direct' : le projet est déployé depuis une image Docker déjà construite.
-- 'github' : le projet est déployé depuis un dépôt de code source GitHub.
CREATE TYPE project_source_type AS ENUM ('direct', 'github');

-- Table principale contenant toutes les informations sur les projets déployés.
CREATE TABLE projects
(
    id SERIAL PRIMARY KEY,

    -- Nom du projet, choisi par l'utilisateur. Doit être unique dans l'application.
    -- Utilisé pour l'affichage et pour générer des noms de domaine (ex: nom-projet.domaine.com).
    name VARCHAR(63) NOT NULL UNIQUE,

    -- Le login de l'utilisateur qui possède le projet et a les droits de gestion principaux.
    owner VARCHAR(255) NOT NULL,

    -- Nom technique unique du conteneur Docker sur la machine hôte.
    -- Il est généré en interne pour éviter les conflits.
    container_name VARCHAR(255) NOT NULL UNIQUE,

    -- Le type de source du projet, utilisant le type personnalisé 'project_source_type'.
    source_type project_source_type NOT NULL,

    -- L'URL de la source. Pour un type 'direct', c'est l'URL de l'image Docker.
    -- Pour un type 'github', c'est l'URL du dépôt Git.
    source_url VARCHAR(2048) NOT NULL,

    -- La branche Git à cloner. N'est utilisé que si 'source_type' est 'github'.
    -- Si NULL, la branche par défaut du dépôt est utilisée.
    source_branch VARCHAR(255) NULL,

    -- Le sous-dossier à la racine du dépôt qui contient le code à servir. N'est utilisé que pour 'github'.
    -- Si NULL, la racine du dépôt est utilisée. Ex: 'src', 'public'.
    source_root_dir VARCHAR(255) NULL,

    -- Le tag complet de l'image Docker qui est actuellement déployée pour ce projet.
    -- Ex: 'nginx:latest' ou 'hangar-local/mon-projet:1678912345'.
    deployed_image_tag VARCHAR(2048) NOT NULL,

    -- L'empreinte (hash SHA256) unique et immuable de l'image Docker déployée.
    -- Essentiel pour vérifier si une nouvelle image est différente de l'actuelle et pour un nettoyage fiable.
    deployed_image_digest VARCHAR(2048) NOT NULL,

    -- Variables d'environnement pour le projet, stockées au format JSON.
    -- Les valeurs sont chiffrées par l'application avant d'être insérées en base.
    env_vars JSONB NULL,

    -- Chemin absolu *à l'intérieur du conteneur* où un volume de données persistant doit être monté.
    -- Ex: '/var/www/html/uploads'.
    persistent_volume_path VARCHAR(2048) NULL,

    -- Nom du volume Docker sur la machine hôte, qui est lié au 'persistent_volume_path'.
    -- Nécessaire pour pouvoir supprimer le bon volume lors de la purge du projet.
    volume_name VARCHAR(255) NULL,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_projects_owner ON projects(owner);

-- Table de liaison (many-to-many) pour gérer les participants à un projet.
CREATE TABLE project_participants
(
    -- Référence à l'identifiant du projet (clé étrangère). Si le projet est supprimé, les participations le sont aussi.
    project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,

    -- Le login de l'utilisateur qui participe au projet.
    participant_id VARCHAR(10) NOT NULL,

    PRIMARY KEY (project_id, participant_id)
);

CREATE INDEX idx_project_participants_participant_id ON project_participants(participant_id);


-- Table pour gérer les bases de données MariaDB provisionnées pour les utilisateurs.
CREATE TABLE databases
(
    id SERIAL PRIMARY KEY,

    -- Le login de l'utilisateur qui possède la base de données.
    owner_login VARCHAR(255) NOT NULL UNIQUE,

    -- Le nom de la base de données sur le serveur MariaDB. Doit être unique.
    database_name VARCHAR(255) NOT NULL UNIQUE,

    -- Le nom d'utilisateur pour se connecter à cette base de données. Doit être unique.
    username VARCHAR(255) NOT NULL UNIQUE,

    -- Le mot de passe de l'utilisateur de la base de données, stocké sous forme chiffrée.
    encrypted_password TEXT NOT NULL,

    -- Référence optionnelle à un projet. Permet de lier une base de données à un projet.
    -- IMPORTANT: La suppression de la base de données MariaDB est gérée par la logique applicative.
    -- ON DELETE SET NULL agit comme une sécurité : si la suppression du projet échoue en cours de route,
    -- le lien est rompu mais la métadonnée de la base de données est conservée, évitant un état incohérent.
    project_id INTEGER NULL REFERENCES projects(id) ON DELETE SET NULL,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_databases_owner_login ON databases(owner_login);