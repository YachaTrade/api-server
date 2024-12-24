pub fn get_env(key: &str) -> String {
    std::env::var(key).unwrap()
}

#[derive(Debug, Clone)]
pub struct DBEnv {
    // Primary DB
    pub primary_db_user: String,
    pub primary_db_password: String,
    pub primary_db_name: String,
    pub primary_db_host: String,
    pub primary_db_port: String,

    // Replica DB
    pub replica_db_user: String,
    pub replica_db_password: String,
    pub replica_db_name: String,
    pub replica_db_host: String,
    pub replica_db_port: String,
}

impl DBEnv {
    pub fn new() -> Self {
        DBEnv {
            // Primary DB
            primary_db_user: get_env("PRIMARY_DB_USER"),
            primary_db_password: get_env("PRIMARY_DB_PASSWORD"),
            primary_db_host: get_env("PRIMARY_DB_HOST"),
            primary_db_port: get_env("PRIMARY_DB_PORT"),
            primary_db_name: get_env("PRIMARY_DB_NAME"),

            // Replica DB
            replica_db_user: get_env("REPLICA_DB_USER"),
            replica_db_password: get_env("REPLICA_DB_PASSWORD"),
            replica_db_host: get_env("REPLICA_DB_HOST"),
            replica_db_port: get_env("REPLICA_DB_PORT"),
            replica_db_name: get_env("REPLICA_DB_NAME"),
        }
    }
}
