//! Núcleo do Finanças Local: banco SQLite, regras de negócio, API e lembretes.
//! Não depende do Tauri, para poder ser testado sozinho (`cargo test -p financas-core`).
pub mod api;
pub mod dates;
pub mod db;
pub mod reminders;

pub use db::Db;

/// Dados vindos do Organizze (seed_version 2), embutidos no executável.
pub const SEED_JSON: &str = include_str!("../seed/dados_iniciais.json");

pub fn seed() -> serde_json::Map<String, serde_json::Value> {
    serde_json::from_str(SEED_JSON).expect("dados_iniciais.json inválido")
}
