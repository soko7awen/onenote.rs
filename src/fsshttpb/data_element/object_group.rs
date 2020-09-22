use crate::fsshttpb::data_element::value::DataElementValue;
use crate::types::binary_item::BinaryItem;
use crate::types::cell_id::CellId;
use crate::types::compact_u64::CompactU64;
use crate::types::exguid::ExGuid;
use crate::types::object_types::ObjectType;
use crate::types::stream_object::ObjectHeader;
use crate::Reader;
use std::fmt;
use std::fmt::Formatter;

#[derive(Debug)]
pub(crate) struct ObjectGroup {
    pub(crate) declarations: Vec<ObjectGroupDeclaration>,
    pub(crate) metadata: Vec<ObjectGroupMetadata>,
    pub(crate) objects: Vec<ObjectGroupData>,
}

#[derive(Debug)]
pub(crate) enum ObjectGroupDeclaration {
    Object {
        object_id: ExGuid,
        partition_id: u64,
        data_size: u64,
        object_reference_count: u64,
        cell_reference_count: u64,
    },
    Blob {
        object_id: ExGuid,
        blob_id: ExGuid,
        partition_id: u64,
        object_reference_count: u64,
        cell_reference_count: u64,
    },
}

impl ObjectGroupDeclaration {
    pub(crate) fn partition_id(&self) -> u64 {
        match self {
            ObjectGroupDeclaration::Object { partition_id, .. } => *partition_id,
            ObjectGroupDeclaration::Blob { partition_id, .. } => *partition_id,
        }
    }

    pub(crate) fn object_id(&self) -> ExGuid {
        match self {
            ObjectGroupDeclaration::Object { object_id, .. } => *object_id,
            ObjectGroupDeclaration::Blob { object_id, .. } => *object_id,
        }
    }
}

#[derive(Debug)]
pub(crate) struct ObjectGroupMetadata {
    pub(crate) change_frequency: ObjectChangeFrequency,
}

#[derive(Debug)]
pub(crate) enum ObjectChangeFrequency {
    Unknown = 0,
    Frequent = 1,
    Infrequent = 2,
    Independent = 3,
    Custom = 4,
}

impl ObjectChangeFrequency {
    fn parse(value: u64) -> ObjectChangeFrequency {
        match value {
            x if x == ObjectChangeFrequency::Unknown as u64 => ObjectChangeFrequency::Unknown,
            x if x == ObjectChangeFrequency::Frequent as u64 => ObjectChangeFrequency::Frequent,
            x if x == ObjectChangeFrequency::Infrequent as u64 => ObjectChangeFrequency::Infrequent,
            x if x == ObjectChangeFrequency::Independent as u64 => {
                ObjectChangeFrequency::Independent
            }
            x if x == ObjectChangeFrequency::Custom as u64 => ObjectChangeFrequency::Custom,
            x => panic!("unexpected change frequency: {}", x),
        }
    }
}

pub(crate) enum ObjectGroupData {
    Object {
        group: Vec<ExGuid>,
        cells: Vec<CellId>,
        data: Vec<u8>,
    },
    ObjectExcluded {
        group: Vec<ExGuid>,
        cells: Vec<CellId>,
        size: u64,
    },
    BlobReference {
        objects: Vec<ExGuid>,
        cells: Vec<CellId>,
        blob: ExGuid,
    },
}

struct DebugSize(usize);

impl fmt::Debug for ObjectGroupData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ObjectGroupData::Object { group, cells, data } => f
                .debug_struct("Object")
                .field("group", group)
                .field("cells", cells)
                .field("data", &DebugSize(data.len()))
                .finish(),
            ObjectGroupData::ObjectExcluded { group, cells, size } => f
                .debug_struct("ObjectExcluded")
                .field("group", group)
                .field("cells", cells)
                .field("size", size)
                .finish(),
            ObjectGroupData::BlobReference {
                objects,
                cells,
                blob,
            } => f
                .debug_struct("ObjectExcluded")
                .field("objects", objects)
                .field("cells", cells)
                .field("blob", blob)
                .finish(),
        }
    }
}

impl fmt::Debug for DebugSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} bytes", self.0)
    }
}

impl DataElementValue {
    pub(crate) fn parse_object_group(reader: Reader) -> DataElementValue {
        let declarations = DataElementValue::parse_object_group_declarations(reader);

        let mut metadata = vec![];

        let object_header = ObjectHeader::parse(reader);
        match object_header.object_type {
            ObjectType::ObjectGroupMetadataBlock => {
                metadata = DataElementValue::parse_object_group_metadata(reader);

                // Parse object header for the group data section
                let object_header = ObjectHeader::parse(reader);
                assert_eq!(object_header.object_type, ObjectType::ObjectGroupData);
            }
            ObjectType::ObjectGroupData => {} // Skip, will be parsed below
            _ => panic!("unexpected object type: 0x{:x}", object_header.object_type),
        }
        let objects = DataElementValue::parse_object_group_data(reader);

        assert_eq!(ObjectHeader::parse_end_8(reader), ObjectType::DataElement);

        DataElementValue::ObjectGroup(ObjectGroup {
            declarations,
            metadata,
            objects,
        })
    }

    fn parse_object_group_declarations(reader: Reader) -> Vec<ObjectGroupDeclaration> {
        let object_header = ObjectHeader::parse(reader);
        assert_eq!(
            object_header.object_type,
            ObjectType::ObjectGroupDeclaration
        );

        let mut declarations = vec![];

        loop {
            if ObjectHeader::try_parse_end_8(reader, ObjectType::ObjectGroupDeclaration).is_some() {
                break;
            }

            let object_header = ObjectHeader::parse(reader);
            match object_header.object_type {
                ObjectType::ObjectGroupObject => {
                    let object_id = ExGuid::parse(reader);
                    let partition_id = CompactU64::parse(reader).value();
                    let data_size = CompactU64::parse(reader).value();
                    let object_reference_count = CompactU64::parse(reader).value();
                    let cell_reference_count = CompactU64::parse(reader).value();

                    declarations.push(ObjectGroupDeclaration::Object {
                        object_id,
                        partition_id,
                        data_size,
                        object_reference_count,
                        cell_reference_count,
                    })
                }
                ObjectType::ObjectGroupDataBlob => {
                    let object_id = ExGuid::parse(reader);
                    let blob_id = ExGuid::parse(reader);
                    let partition_id = CompactU64::parse(reader).value();
                    let object_reference_count = CompactU64::parse(reader).value();
                    let cell_reference_count = CompactU64::parse(reader).value();

                    declarations.push(ObjectGroupDeclaration::Blob {
                        object_id,
                        blob_id,
                        partition_id,
                        object_reference_count,
                        cell_reference_count,
                    })
                }
                _ => panic!("unexpected object type: 0x{:x}", object_header.object_type),
            }
        }

        declarations
    }

    fn parse_object_group_metadata(reader: Reader) -> Vec<ObjectGroupMetadata> {
        let mut declarations = vec![];

        loop {
            if ObjectHeader::try_parse_end_8(reader, ObjectType::ObjectGroupMetadataBlock).is_some()
            {
                break;
            }

            let object_header = ObjectHeader::parse_32(reader);
            assert_eq!(object_header.object_type, ObjectType::ObjectGroupMetadata);

            let frequency = CompactU64::parse(reader);
            declarations.push(ObjectGroupMetadata {
                change_frequency: ObjectChangeFrequency::parse(frequency.value()),
            })
        }

        declarations
    }

    fn parse_object_group_data(reader: Reader) -> Vec<ObjectGroupData> {
        let mut objects = vec![];

        loop {
            if ObjectHeader::try_parse_end_8(reader, ObjectType::ObjectGroupData).is_some() {
                break;
            }

            let object_header = ObjectHeader::parse(reader);
            match object_header.object_type {
                ObjectType::ObjectGroupDataExcluded => {
                    let group = ExGuid::parse_array(reader);
                    let cells = CellId::parse_array(reader);
                    let size = CompactU64::parse(reader).value();

                    objects.push(ObjectGroupData::ObjectExcluded { group, cells, size })
                }
                ObjectType::ObjectGroupDataObject => {
                    let group = ExGuid::parse_array(reader);
                    let cells = CellId::parse_array(reader);
                    let data = BinaryItem::parse(reader).value();

                    objects.push(ObjectGroupData::Object { group, cells, data })
                }
                ObjectType::ObjectGroupBlobReference => {
                    let references = ExGuid::parse_array(reader);
                    let cells = CellId::parse_array(reader);
                    let blob = ExGuid::parse(reader);

                    objects.push(ObjectGroupData::BlobReference {
                        objects: references,
                        cells,
                        blob,
                    })
                }
                _ => panic!("unexpected object type: 0x{:x}", object_header.object_type),
            }
        }

        objects
    }
}
