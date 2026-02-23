use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

#[derive(Debug)]
pub enum AppError {
    Sqlx(sqlx::Error),
    Anyhow(anyhow::Error),
    NotFound(String),
    BadRequest(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::Sqlx(e) => {
                tracing::error!("Database error: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Database error: {e}"))
            }
            AppError::Anyhow(e) => {
                tracing::error!("Internal error: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Internal error: {e}"))
            }
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
        };
        (status, message).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::Sqlx(e)
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::Anyhow(e)
    }
}

pub type Result<T> = std::result::Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;

    #[test]
    fn bad_request_yields_400() {
        let resp = AppError::BadRequest("bad input".to_string()).into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn not_found_yields_404() {
        let resp = AppError::NotFound("missing".to_string()).into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn anyhow_error_yields_500() {
        let resp = AppError::Anyhow(anyhow::anyhow!("boom")).into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn from_anyhow_conversion() {
        let anyhow_err: anyhow::Error = anyhow::anyhow!("test error");
        let app_err = AppError::from(anyhow_err);
        assert!(matches!(app_err, AppError::Anyhow(_)));
    }

    #[test]
    fn from_sqlx_conversion() {
        // sqlx::Error::RowNotFound is easy to construct
        let sqlx_err = sqlx::Error::RowNotFound;
        let app_err = AppError::from(sqlx_err);
        assert!(matches!(app_err, AppError::Sqlx(_)));
        let resp = app_err.into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn result_type_alias_works() {
        let ok: Result<i32> = Ok(42);
        assert_eq!(ok.unwrap(), 42);

        let err: Result<i32> = Err(AppError::BadRequest("x".into()));
        assert!(err.is_err());
    }
}
