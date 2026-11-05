use noteiq_core::launch::{LaunchError, LaunchRequest, parse_launch_request};

pub fn parse_arguments(arguments: &[String]) -> Result<LaunchRequest, LaunchError> {
    match arguments {
        [_program] => Ok(LaunchRequest::NewNote),
        [_, value] if is_noteiq_uri(value) => LaunchRequest::from_uri(value),
        _ => parse_launch_request(arguments),
    }
}

pub fn is_noteiq_uri(value: &str) -> bool {
    value
        .split_once(':')
        .is_some_and(|(scheme, _)| scheme.eq_ignore_ascii_case("noteiq"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_shell_uri_routes_are_case_insensitive() {
        for uri in ["noteiq://new", "NOTEIQ://NEW/", "NoteIQ://new"] {
            assert!(is_noteiq_uri(uri));
            assert_eq!(
                parse_arguments(&["noteiq".into(), uri.into()]),
                Ok(LaunchRequest::NewNote)
            );
        }
        assert!(!is_noteiq_uri("new"));
        assert!(!is_noteiq_uri("https://new"));
        assert!(parse_arguments(&["noteiq".into(), "NOTEIQ://wrong".into()]).is_err());
    }
}
