use std::io;

#[inline]
pub fn read_file_to_string<P>(path: P) -> Result<String, io::Error>
where
    P: AsRef<std::path::Path>,
{
    #[cfg(feature = "snelleutf")]
    return snelleutf::utils::read_file_to_string(path);
    #[cfg(not(feature = "snelleutf"))]
    return std::fs::read_to_string(path);
}
