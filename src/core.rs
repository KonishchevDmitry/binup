pub type EmptyResult = GenericResult<()>;
pub type GenericResult<T> = Result<T, GenericError>;
pub type GenericError = Box<dyn ::std::error::Error + Send + Sync>;

#[macro_export]
macro_rules! Err {
    ($($arg:tt)*) => (::std::result::Result::Err(format!($($arg)*).into()))
}