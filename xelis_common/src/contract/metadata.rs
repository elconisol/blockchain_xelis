use xelis_vm::Module;
use crate::serializer::*;

/// Metadata container for a smart contract.
/// Wraps the VM `Module` and provides serialization support.
#[derive(Debug)]
pub struct ContractMetadata {
    pub module: Module,
}

impl Serializer for ContractMetadata {
    fn write(&self, writer: &mut Writer) {
        // Delegate writing to the underlying Module
        self.module.write(writer);
    }

    fn read(reader: &mut Reader) -> Result<Self, ReaderError> {
        // Read back a Module from the stream
        let module = Module::read(reader)?;
        Ok(Self { module })
    }

    fn size(&self) -> usize {
        // Size equals the size of the inner Module
        self.module.size()
    }
}
