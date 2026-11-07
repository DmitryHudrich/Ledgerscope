use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueError {
    field: &'static str,
    reason: String,
}

impl ValueError {
    fn new(field: &'static str, reason: impl Into<String>) -> Self {
        Self {
            field,
            reason: reason.into(),
        }
    }

    pub fn field(&self) -> &'static str {
        self.field
    }
}

impl fmt::Display for ValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.field, self.reason)
    }
}

impl std::error::Error for ValueError {}

fn bounded(field: &'static str, raw: &str, min: usize, max: usize) -> Result<String, ValueError> {
    let trimmed = raw.trim();
    let length = trimmed.chars().count();
    if length < min {
        return Err(ValueError::new(
            field,
            if min == 1 {
                "must not be empty".to_owned()
            } else {
                format!("must be at least {min} characters")
            },
        ));
    }
    if length > max {
        return Err(ValueError::new(
            field,
            format!("must be at most {max} characters"),
        ));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(ValueError::new(field, "must not hold control characters"));
    }
    Ok(trimmed.to_owned())
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Email(String);

impl Email {
    const MAX: usize = 254;

    pub fn parse(raw: &str) -> Result<Self, ValueError> {
        let email = raw.trim().to_lowercase();
        let invalid = || ValueError::new("email", "is not a valid address");

        if email.is_empty() || email.len() > Self::MAX || email.chars().any(char::is_whitespace) {
            return Err(invalid());
        }

        let (local, domain) = email.split_once('@').ok_or_else(invalid)?;
        let domain_ok = domain.contains('.')
            && !domain.starts_with('.')
            && !domain.ends_with('.')
            && !domain.contains("..")
            && !domain.contains('@');
        if local.is_empty() || !domain_ok {
            return Err(invalid());
        }

        Ok(Self(email))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Email {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

pub struct Password(String);

impl Password {
    const MIN: usize = 10;
    const MAX: usize = 128;

    pub fn parse(raw: &str) -> Result<Self, ValueError> {
        let length = raw.chars().count();
        if length < Self::MIN {
            return Err(ValueError::new(
                "password",
                format!("must be at least {} characters", Self::MIN),
            ));
        }
        if length > Self::MAX {
            return Err(ValueError::new(
                "password",
                format!("must be at most {} characters", Self::MAX),
            ));
        }
        if raw.trim().is_empty() {
            return Err(ValueError::new("password", "must not be blank"));
        }
        Ok(Self(raw.to_owned()))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Password(***)")
    }
}

macro_rules! text {
    ($name:ident, $field:literal, $min:expr, $max:expr) => {
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct $name(String);

        impl $name {
            pub fn parse(raw: &str) -> Result<Self, ValueError> {
                bounded($field, raw, $min, $max).map(Self)
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn into_inner(self) -> String {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

text!(DisplayName, "name", 1, 64);
text!(TeamName, "team name", 1, 64);
text!(CanvasTitle, "title", 1, 120);
text!(LabelValue, "label", 1, 64);
text!(LabelNote, "note", 0, 500);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_email_is_trimmed_and_lowercased() {
        assert_eq!(
            Email::parse("  Alice@Example.COM ").unwrap().as_str(),
            "alice@example.com"
        );
    }

    #[test]
    fn a_broken_email_is_turned_away() {
        for raw in [
            "",
            "alice",
            "alice@",
            "@example.com",
            "alice@example",
            "a b@example.com",
            "a@b@c.com",
            "a@.com",
            "a@example..com",
        ] {
            assert!(Email::parse(raw).is_err(), "{raw} passed");
        }
    }

    #[test]
    fn a_short_password_is_turned_away() {
        assert!(Password::parse("short").is_err());
        assert!(Password::parse("          ").is_err());
        assert!(Password::parse("long enough password").is_ok());
    }

    #[test]
    fn a_password_never_shows_in_debug() {
        assert_eq!(
            format!("{:?}", Password::parse("hunter4242").unwrap()),
            "Password(***)"
        );
    }

    #[test]
    fn text_is_trimmed_and_bounded() {
        assert_eq!(
            LabelValue::parse("  Binance hot ").unwrap().as_str(),
            "Binance hot"
        );
        assert!(LabelValue::parse("   ").is_err());
        assert!(LabelValue::parse(&"x".repeat(65)).is_err());
        assert!(LabelNote::parse("").is_ok());
        assert!(TeamName::parse("bad\u{7}name").is_err());
    }
}
