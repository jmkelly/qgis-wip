//! Point cloud attribute types.
//!
//! Matches QGIS `QgsPointCloudAttribute` which describes a single
//! attribute (field) of a point cloud, such as X, Y, Z, intensity,
//! return number, etc.

/// The data type of a point cloud attribute.
///
/// Matches QGIS `QgsPointCloudAttribute::DataType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PointCloudAttributeType {
    /// 8-bit signed integer.
    Int8,
    /// 8-bit unsigned integer.
    UInt8,
    /// 16-bit signed integer.
    Int16,
    /// 16-bit unsigned integer.
    UInt16,
    /// 32-bit signed integer.
    Int32,
    /// 32-bit unsigned integer.
    UInt32,
    /// 64-bit signed integer.
    Int64,
    /// 64-bit unsigned integer.
    UInt64,
    /// 32-bit floating point.
    Float32,
    /// 64-bit floating point.
    Float64,
}

impl PointCloudAttributeType {
    /// Returns the size in bytes of this attribute type.
    pub fn size_bytes(self) -> usize {
        match self {
            PointCloudAttributeType::Int8 | PointCloudAttributeType::UInt8 => 1,
            PointCloudAttributeType::Int16 | PointCloudAttributeType::UInt16 => 2,
            PointCloudAttributeType::Int32
            | PointCloudAttributeType::UInt32
            | PointCloudAttributeType::Float32 => 4,
            PointCloudAttributeType::Int64
            | PointCloudAttributeType::UInt64
            | PointCloudAttributeType::Float64 => 8,
        }
    }

    /// Returns true if this is a floating-point type.
    pub fn is_floating(self) -> bool {
        matches!(
            self,
            PointCloudAttributeType::Float32 | PointCloudAttributeType::Float64
        )
    }

    /// Returns true if this is a signed integer type.
    pub fn is_signed(self) -> bool {
        matches!(
            self,
            PointCloudAttributeType::Int8
                | PointCloudAttributeType::Int16
                | PointCloudAttributeType::Int32
                | PointCloudAttributeType::Int64
        )
    }

    /// Returns true if this is an unsigned integer type.
    pub fn is_unsigned(self) -> bool {
        matches!(
            self,
            PointCloudAttributeType::UInt8
                | PointCloudAttributeType::UInt16
                | PointCloudAttributeType::UInt32
                | PointCloudAttributeType::UInt64
        )
    }
}

/// A point cloud attribute definition.
///
/// Matches QGIS `QgsPointCloudAttribute`. Describes a named attribute
/// with a type, size, and optional description.
#[derive(Debug, Clone, PartialEq)]
pub struct PointCloudAttribute {
    /// Attribute name (e.g., "X", "Y", "Z", "Intensity", "ReturnNumber").
    pub name: String,
    /// Attribute data type.
    pub attribute_type: PointCloudAttributeType,
    /// Number of bytes per point for this attribute.
    pub size: usize,
    /// Optional description of the attribute.
    pub description: Option<String>,
}

impl PointCloudAttribute {
    /// Creates a new point cloud attribute.
    pub fn new(
        name: impl Into<String>,
        attribute_type: PointCloudAttributeType,
    ) -> Self {
        let size = attribute_type.size_bytes();
        Self {
            name: name.into(),
            attribute_type,
            size,
            description: None,
        }
    }

    /// Creates a new point cloud attribute with a description.
    pub fn with_description(
        name: impl Into<String>,
        attribute_type: PointCloudAttributeType,
        description: impl Into<String>,
    ) -> Self {
        let mut attr = Self::new(name, attribute_type);
        attr.description = Some(description.into());
        attr
    }

    /// Returns the attribute name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the attribute type.
    pub fn attribute_type(&self) -> PointCloudAttributeType {
        self.attribute_type
    }

    /// Returns the size in bytes per point.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Returns the description, if any.
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// Creates the standard LAS X attribute.
    pub fn las_x() -> Self {
        Self::new("X", PointCloudAttributeType::Int32)
    }

    /// Creates the standard LAS Y attribute.
    pub fn las_y() -> Self {
        Self::new("Y", PointCloudAttributeType::Int32)
    }

    /// Creates the standard LAS Z attribute.
    pub fn las_z() -> Self {
        Self::new("Z", PointCloudAttributeType::Int32)
    }

    /// Creates the standard LAS Intensity attribute.
    pub fn intensity() -> Self {
        Self::new("Intensity", PointCloudAttributeType::UInt16)
    }

    /// Creates the standard LAS ReturnNumber attribute.
    pub fn return_number() -> Self {
        Self::new("ReturnNumber", PointCloudAttributeType::UInt8)
    }

    /// Creates the standard LAS NumberOfReturns attribute.
    pub fn number_of_returns() -> Self {
        Self::new("NumberOfReturns", PointCloudAttributeType::UInt8)
    }

    /// Creates the standard LAS Classification attribute.
    pub fn classification() -> Self {
        Self::new("Classification", PointCloudAttributeType::UInt8)
    }

    /// Creates the standard LAS ScanAngleRank attribute.
    pub fn scan_angle_rank() -> Self {
        Self::new("ScanAngleRank", PointCloudAttributeType::Int8)
    }

    /// Creates the standard LAS UserData attribute.
    pub fn user_data() -> Self {
        Self::new("UserData", PointCloudAttributeType::UInt8)
    }

    /// Creates the standard LAS PointSourceId attribute.
    pub fn point_source_id() -> Self {
        Self::new("PointSourceId", PointCloudAttributeType::UInt16)
    }

    /// Creates the standard LAS GpsTime attribute.
    pub fn gps_time() -> Self {
        Self::new("GpsTime", PointCloudAttributeType::Float64)
    }

    /// Creates the standard LAS Red attribute.
    pub fn red() -> Self {
        Self::new("Red", PointCloudAttributeType::UInt16)
    }

    /// Creates the standard LAS Green attribute.
    pub fn green() -> Self {
        Self::new("Green", PointCloudAttributeType::UInt16)
    }

    /// Creates the standard LAS Blue attribute.
    pub fn blue() -> Self {
        Self::new("Blue", PointCloudAttributeType::UInt16)
    }
}

/// The standard LAS attribute set.
///
/// Matches the default attribute set in QGIS `QgsPointCloudLayer`.
pub fn standard_las_attributes() -> Vec<PointCloudAttribute> {
    vec![
        PointCloudAttribute::las_x(),
        PointCloudAttribute::las_y(),
        PointCloudAttribute::las_z(),
        PointCloudAttribute::intensity(),
        PointCloudAttribute::return_number(),
        PointCloudAttribute::number_of_returns(),
        PointCloudAttribute::classification(),
        PointCloudAttribute::scan_angle_rank(),
        PointCloudAttribute::user_data(),
        PointCloudAttribute::point_source_id(),
        PointCloudAttribute::gps_time(),
        PointCloudAttribute::red(),
        PointCloudAttribute::green(),
        PointCloudAttribute::blue(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attribute_type_size() {
        assert_eq!(PointCloudAttributeType::Int8.size_bytes(), 1);
        assert_eq!(PointCloudAttributeType::UInt8.size_bytes(), 1);
        assert_eq!(PointCloudAttributeType::Int16.size_bytes(), 2);
        assert_eq!(PointCloudAttributeType::UInt16.size_bytes(), 2);
        assert_eq!(PointCloudAttributeType::Int32.size_bytes(), 4);
        assert_eq!(PointCloudAttributeType::UInt32.size_bytes(), 4);
        assert_eq!(PointCloudAttributeType::Float32.size_bytes(), 4);
        assert_eq!(PointCloudAttributeType::Int64.size_bytes(), 8);
        assert_eq!(PointCloudAttributeType::UInt64.size_bytes(), 8);
        assert_eq!(PointCloudAttributeType::Float64.size_bytes(), 8);
    }

    #[test]
    fn attribute_type_flags() {
        assert!(PointCloudAttributeType::Float32.is_floating());
        assert!(PointCloudAttributeType::Float64.is_floating());
        assert!(!PointCloudAttributeType::Int32.is_floating());

        assert!(PointCloudAttributeType::Int8.is_signed());
        assert!(PointCloudAttributeType::Int16.is_signed());
        assert!(PointCloudAttributeType::Int32.is_signed());
        assert!(PointCloudAttributeType::Int64.is_signed());
        assert!(!PointCloudAttributeType::UInt8.is_signed());

        assert!(PointCloudAttributeType::UInt8.is_unsigned());
        assert!(PointCloudAttributeType::UInt16.is_unsigned());
        assert!(PointCloudAttributeType::UInt32.is_unsigned());
        assert!(PointCloudAttributeType::UInt64.is_unsigned());
        assert!(!PointCloudAttributeType::Int8.is_unsigned());
    }

    #[test]
    fn point_cloud_attribute_new() {
        let attr = PointCloudAttribute::new("X", PointCloudAttributeType::Int32);
        assert_eq!(attr.name(), "X");
        assert_eq!(attr.attribute_type(), PointCloudAttributeType::Int32);
        assert_eq!(attr.size(), 4);
        assert_eq!(attr.description(), None);
    }

    #[test]
    fn point_cloud_attribute_with_description() {
        let attr = PointCloudAttribute::with_description(
            "Intensity",
            PointCloudAttributeType::UInt16,
            "Laser return intensity",
        );
        assert_eq!(attr.name(), "Intensity");
        assert_eq!(attr.description(), Some("Laser return intensity"));
    }

    #[test]
    fn las_standard_attributes() {
        let attrs = standard_las_attributes();
        assert_eq!(attrs.len(), 14);
        assert_eq!(attrs[0].name(), "X");
        assert_eq!(attrs[1].name(), "Y");
        assert_eq!(attrs[2].name(), "Z");
        assert_eq!(attrs[3].name(), "Intensity");
        assert_eq!(attrs[6].name(), "Classification");
        assert_eq!(attrs[10].name(), "GpsTime");
        assert_eq!(attrs[13].name(), "Blue");
    }

    #[test]
    fn individual_las_attributes() {
        assert_eq!(PointCloudAttribute::las_x().name(), "X");
        assert_eq!(PointCloudAttribute::las_y().name(), "Y");
        assert_eq!(PointCloudAttribute::las_z().name(), "Z");
        assert_eq!(PointCloudAttribute::intensity().name(), "Intensity");
        assert_eq!(PointCloudAttribute::return_number().name(), "ReturnNumber");
        assert_eq!(PointCloudAttribute::number_of_returns().name(), "NumberOfReturns");
        assert_eq!(PointCloudAttribute::classification().name(), "Classification");
        assert_eq!(PointCloudAttribute::gps_time().name(), "GpsTime");
        assert_eq!(PointCloudAttribute::red().name(), "Red");
        assert_eq!(PointCloudAttribute::green().name(), "Green");
        assert_eq!(PointCloudAttribute::blue().name(), "Blue");
    }

    #[test]
    fn las_attribute_sizes() {
        assert_eq!(PointCloudAttribute::las_x().size(), 4);
        assert_eq!(PointCloudAttribute::las_y().size(), 4);
        assert_eq!(PointCloudAttribute::las_z().size(), 4);
        assert_eq!(PointCloudAttribute::intensity().size(), 2);
        assert_eq!(PointCloudAttribute::classification().size(), 1);
        assert_eq!(PointCloudAttribute::gps_time().size(), 8);
        assert_eq!(PointCloudAttribute::red().size(), 2);
    }
}
