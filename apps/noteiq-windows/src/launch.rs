use noteiq_core::launch::{LaunchError, LaunchRequest, parse_launch_request};

pub fn parse_arguments(arguments: &[String]) -> Result<LaunchRequest, LaunchError> {
    match arguments {
        [_program] => Ok(LaunchRequest::NewNote),
        [_, value] if value.starts_with("noteiq:") => LaunchRequest::from_uri(value),
        _ => parse_launch_request(arguments),
    }
}
