use std::io;
use monoio::io::{AsyncReadRent, AsyncWriteRent, AsyncWriteRentExt};

/// inspired by monoio::io::utils::copy
/// but with configurable buf size
pub async fn copy<'a, R, W>(reader: &'a mut R, writer: &'a mut W, buf_size: usize) -> io::Result<u64>
where
    R: AsyncReadRent + ?Sized,
    W: AsyncWriteRent + ?Sized,
{
    let mut buf: Vec<u8> = Vec::with_capacity(buf_size);
    let mut transferred: u64 = 0;

    loop {
        let (read_res, mut buf_read) = reader.read(buf).await;
        match read_res {
            Ok(0) => {
                // read closed
                break;
            }
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {
                // retry
                buf = buf_read;
                continue;
            }
            Err(e) => {
                // should return error
                return Err(e);
            }
            Ok(_) => {
                // go write data
            }
        }

        'w: loop {
            let (write_res, buf_) = writer.write_all(buf_read).await;
            match write_res {
                Ok(0) => {
                    // write closed
                    return Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "write zero byte into writer",
                    ));
                }
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {
                    // retry
                    buf_read = buf_;
                    continue 'w;
                }
                Err(e) => {
                    // should return error
                    return Err(e);
                }
                Ok(n) => {
                    // go read data
                    transferred += n as u64;
                    buf = buf_;
                    break;
                }
            }
        }
    }

    Ok(transferred)
}