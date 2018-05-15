use io::{Write, ProtoWrite, Error};

#[derive(Debug)]
pub struct Header {
    pub sent_bytes: u32,
    pub total_byte_count: u64,
    pub overflow_occurred: bool,
    pub log_channel: u8,
    pub dds_onehot_sel: bool
}

impl Header {
    pub fn write_to<T: Write>(&self, writer: &mut T) -> Result<(), Error<T::WriteError>> {
        writer.write_u32(self.sent_bytes)?;
        writer.write_u64(self.total_byte_count)?;
        writer.write_u8(self.overflow_occurred as u8)?;
        writer.write_u8(self.log_channel)?;
        writer.write_u8(self.dds_onehot_sel as u8)?;
        Ok(())
    }
}
