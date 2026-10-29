use envconfig::Envconfig;

#[derive(Envconfig)]
pub struct Env {
    #[envconfig(from = "HOST", default = "127.0.0.1")]
    pub host: String,

    #[envconfig(from = "PORT", default = "8080")]
    pub port: usize,
}

impl Env {
    pub fn env_init() -> Env {
        return Env::init_from_env().unwrap();
    }
}
