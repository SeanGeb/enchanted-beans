/// Types implementing BeanstalkResponse can be sent over the Beanstalk TCP
/// connection in the client -> server connection.
pub trait BeanstalkSerialisable {
    /// Converts the value in question to a Beanstalk command or response.
    fn serialise_beanstalk(&self) -> Vec<u8>;
}
