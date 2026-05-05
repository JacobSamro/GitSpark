pub(crate) fn stable_id_slug(value: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            previous_dash = false;
        } else if !previous_dash && !slug.is_empty() {
            slug.push('-');
            previous_dash = true;
        }
    }

    while slug.ends_with('-') {
        slug.pop();
    }

    if slug.is_empty() {
        "item".to_string()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::stable_id_slug;

    #[test]
    fn creates_stable_id_slugs() {
        assert_eq!(stable_id_slug("src/main.rs"), "src-main-rs");
        assert_eq!(stable_id_slug("feature/login-ui"), "feature-login-ui");
        assert_eq!(stable_id_slug("..."), "item");
    }
}
