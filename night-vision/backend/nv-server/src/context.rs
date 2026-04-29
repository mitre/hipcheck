use sea_orm::DatabaseConnection;

pub struct ApiCtx {
    pub db: DatabaseConnection,
}
