use sea_orm::DatabaseConnection;

pub struct ApiCtx {
    #[allow(unused)]
    pub db: DatabaseConnection,
}
