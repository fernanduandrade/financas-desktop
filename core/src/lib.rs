//! Núcleo do Finanças Local: banco SQLite, regras de negócio, API e lembretes.
//! Não depende do Tauri, para poder ser testado sozinho (`cargo test -p financas-core`).
//!
//! O app não traz dados embutidos: numa instalação nova o banco começa vazio, e quem já tem
//! dados (versão Python ou versão anterior) continua com o mesmo banco.
pub mod api;
pub mod dates;
pub mod db;
pub mod reminders;

pub use db::Db;
