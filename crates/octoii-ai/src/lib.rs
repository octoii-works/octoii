//! Octoii's access to models: one vocabulary of conversations, in and out, whichever provider serves the model; the
//! catalogue of models and what each is; and each provider's driver, which renders a request for its provider and
//! reads its answer back.

pub mod catalogue;
pub mod driver;
pub mod drivers;
pub mod host;
pub mod models;
pub mod protocols;
pub mod stream;
pub mod transport;
pub mod vocabulary;
pub mod wire;
