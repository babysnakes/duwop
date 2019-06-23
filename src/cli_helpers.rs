use failure::Error;
use log::error;
use structopt::clap::arg_enum;

arg_enum! {
    #[derive(Debug, PartialEq)]
    pub enum LogCommand {
        Debug,
        Trace,
        Reset,
        Custom,
    }
}

pub fn print_full_error(err: Error) {
    error!("{}", err);
    for cause in err.iter_causes() {
        error!("{}", cause);
    }
}
