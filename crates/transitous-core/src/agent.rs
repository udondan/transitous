//! The User-Agent. Transitous asks every client to name a contact in it
//! (<https://transitous.org/api/>), so there is no request without one.

use crate::Error;

/// The project, named in every User-Agent.
pub const PROJECT_URL: &str = "https://github.com/udondan/transitous";

/// The User-Agent for `product` (like `transitous/1.2.0`) and the contact.
///
/// # Errors
///
/// [`Error::ContactMissing`] without a contact, [`Error::InvalidContact`] if
/// it is neither a URL nor an email address.
pub fn user_agent(product: &str, contact: Option<&str>) -> Result<String, Error> {
    let contact = contact
        .map(str::trim)
        .filter(|contact| !contact.is_empty())
        .ok_or(Error::ContactMissing)?;
    check_contact(contact)?;
    Ok(format!("{product} (+{PROJECT_URL}; contact: {contact})"))
}

/// Checks that the contact is an `http(s)` or `mailto:` URL, or an email
/// address, and fits in a User-Agent.
pub(crate) fn check_contact(contact: &str) -> Result<(), Error> {
    let invalid = |reason: &str| Err(Error::InvalidContact(reason.to_owned()));
    if contact
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '(' | ')' | ';'))
    {
        return invalid("it must not contain spaces, parentheses or semicolons");
    }
    let lower = contact.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return match motis::Url::parse(contact) {
            Ok(url) if url.host_str().is_some_and(|host| !host.is_empty()) => Ok(()),
            _ => invalid("the URL is not valid"),
        };
    }
    let address = lower.strip_prefix("mailto:").unwrap_or(&lower);
    match address.split_once('@') {
        Some((local, domain))
            if !local.is_empty() && domain.contains('.') && !domain.contains('@') =>
        {
            Ok(())
        }
        _ => invalid("use a URL or an email address"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_product_project_and_contact() {
        assert_eq!(
            user_agent("transitous/1.0.0", Some(" mailto:me@example.org ")).unwrap(),
            "transitous/1.0.0 (+https://github.com/udondan/transitous; contact: mailto:me@example.org)"
        );
    }

    #[test]
    fn needs_a_contact() {
        for contact in [None, Some(""), Some("  ")] {
            assert!(matches!(
                user_agent("t/1", contact),
                Err(Error::ContactMissing)
            ));
        }
    }

    #[test]
    fn accepts_urls_and_addresses() {
        for contact in [
            "https://github.com/me/bot",
            "HTTP://example.org",
            "me@example.org",
            "mailto:me@example.org",
        ] {
            check_contact(contact).unwrap_or_else(|e| panic!("{contact}: {e}"));
        }
    }

    #[test]
    fn rejects_other_contacts() {
        for contact in [
            "me",
            "@example.org",
            "me@localhost",
            "me@a@b.org",
            "https://",
            "https://exa mple.org",
            "me@example.org)",
            "a;b@example.org",
            "mailto:",
        ] {
            assert!(
                matches!(check_contact(contact), Err(Error::InvalidContact(_))),
                "{contact}"
            );
        }
    }
}
