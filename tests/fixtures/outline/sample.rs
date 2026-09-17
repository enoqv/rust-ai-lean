//! Fixture for `outline` tests.

use std::fmt;

/// Maximum retries.
pub const MAX_RETRIES: u32 = 3;

static mut COUNTER: u64 = 0;

pub type Result<T> = std::result::Result<T, Error>;

/// A user record.
#[derive(Debug, Clone)]
pub struct User {
    /// Primary key.
    pub id: String,
    #[allow(dead_code)]
    name: String,
}

pub struct Id(pub String);

pub struct Marker;

pub enum Error {
    NotFound,
    Invalid(String),
    Conflict { expected: u64, actual: u64 },
}

pub union Bits {
    pub int: u32,
    pub float: f32,
}

pub trait Repository: Send + Sync {
    type Item;
    const NAME: &'static str;
    fn get(&self, id: &str) -> Option<Self::Item>;
    fn count(&self) -> usize {
        0
    }
}

impl<T> Repository for Vec<T>
where
    T: Clone + Send + Sync,
{
    type Item = T;
    const NAME: &'static str = "vec";
    fn get(&self, _id: &str) -> Option<T> {
        None
    }
}

impl User {
    pub const KIND: &'static str = "user";

    /// Creates a user.
    pub async fn insert_user(
        conn: &mut Vec<User>,
        id: &str,
        name: &str,
        display_name: Option<&str>,
        email: Option<&str>,
    ) -> Result<User> {
        let user = User { id: id.into(), name: name.into() };
        conn.push(user.clone());
        let _ = (display_name, email);
        Ok(user)
    }
}

impl fmt::Display for User {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id)
    }
}

pub mod nested {
    pub fn helper() {}

    mod deeper {
        pub(crate) fn inner() {}
    }
}

mod external;

macro_rules! square {
    ($x:expr) => {
        $x * $x
    };
}

#[cfg(test)]
mod tests {
    #[test]
    fn one() {}

    #[test]
    fn two() {}
}
