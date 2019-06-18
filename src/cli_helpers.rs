use failure::Error;
use log::error;

pub fn print_full_error(err: Error) {
    error!("{}", err);
    for cause in err.iter_causes() {
        error!("{}", cause);
    }
}
