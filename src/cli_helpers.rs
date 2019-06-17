use clap::ErrorKind;
use failure::Error;
use log::error;

/// A helper around converting _clap_ matches to the requested type with typed
/// default value. It seems redundant because of clap's `value_t` macro, however
/// it offers advantages:
///
/// * _Clap_ currently has an issue (not sure if it's a bug or a feature - I'll
///   open a ticket and we'll see) that you can specify an option that requires
///   an argument without providing the argument if there's a default value
///   which can cause confusion. This way you can provide a default without clap
///   knowing it.
/// * It offers more flexibility on setting the default (e.g. you can decide
///   your default based on other parameter's argument). While you can do it on
///   `value_t` and inspecting the Error, but this is less boilerplate.
/// * Better control over error messages.
pub fn parse_val_with_default<A>(opt: &str, matches: &clap::ArgMatches, default: A) -> A
where
    A: std::str::FromStr,
{
    match matches.value_of(opt) {
        Some(val) => match A::from_str(val) {
            Ok(port) => port,
            Err(_) => {
                let msg = format!(
                    "Invalid value for '{}': '{}' is not a valid value",
                    opt, &val
                );
                let err = clap::Error::with_description(&msg, ErrorKind::InvalidValue);
                err.exit()
            }
        },
        None => default,
    }
}

pub fn print_full_error(err: Error) {
    error!("{}", err);
    for cause in err.iter_causes() {
        error!("{}", cause);
    }
}
