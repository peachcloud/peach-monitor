//! Basic error handling for network and nest.

use std::error;

pub type BoxError = Box<dyn error::Error>;

#[derive(Debug)]
pub enum NetworkError {

}

#[derive(Debug)]
pub enum NestError {

}
