use envconfig::Envconfig;

#[derive(Envconfig)]
pub struct Env {
    #[envconfig(from = "HOST")]
    pub host: String,

    #[envconfig(from = "PORT")]
    pub port: usize,
}

impl Env {
    pub fn env_init() -> Env {
        return Env::init_from_env().unwrap();
    }
}
