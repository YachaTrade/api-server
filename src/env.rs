pub fn get_env(key: &str) -> String {
    // println!("{:?}", key);
    std::env::var(key).unwrap()
}

#[derive(Debug, Clone)]
pub struct DBEnv {
    // pub db_url: String,
    pub user: String,
    pub password: String,
    pub db_name: String,
    pub host: String,
    pub port: String,
}

impl DBEnv {
    pub fn new() -> Self {
        DBEnv {
            // db_url: get_env("DB_URL"),
            user: get_env("DB_USER"),
            password: get_env("DB_PASSWORD"),
            host: get_env("DB_HOST"),
            port: get_env("DB_PORT"),
            db_name: get_env("DB_NAME"),
        }
    }
}
