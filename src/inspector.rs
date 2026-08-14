use mtp_rs::ptp::{ObjectFormatCode, ObjectPropertyCode};

// The mtp-rs high-level API exposes a portable metadata subset. Keep the rows stable so the
// inspector can show which protocol-specific values are unavailable through that API.
pub const INSPECTOR_PROPERTIES: &[ObjectPropertyCode] = &[
    ObjectPropertyCode::StorageId,
    ObjectPropertyCode::ObjectFormat,
    ObjectPropertyCode::ProtectionStatus,
    ObjectPropertyCode::ObjectSize,
    ObjectPropertyCode::ObjectFileName,
    ObjectPropertyCode::DateCreated,
    ObjectPropertyCode::DateModified,
    ObjectPropertyCode::ParentObject,
    ObjectPropertyCode::Name,
];

pub fn prop_name(code: ObjectPropertyCode) -> String {
    match code {
        ObjectPropertyCode::StorageId => "StorageId".into(),
        ObjectPropertyCode::ObjectFormat => "ObjectFormat".into(),
        ObjectPropertyCode::ProtectionStatus => "ProtectionStatus".into(),
        ObjectPropertyCode::ObjectSize => "ObjectSize".into(),
        ObjectPropertyCode::ObjectFileName => "ObjectFileName".into(),
        ObjectPropertyCode::DateCreated => "DateCreated".into(),
        ObjectPropertyCode::DateModified => "DateModified".into(),
        ObjectPropertyCode::ParentObject => "ParentObject".into(),
        ObjectPropertyCode::Name => "Name".into(),
        ObjectPropertyCode::Unknown(c) => format!("0x{c:04X}"),
    }
}

pub fn format_object_format(format: mtp_rs::ObjectFormat) -> String {
    let code = ObjectFormatCode::from(format.code());
    match code {
        ObjectFormatCode::Undefined => "Undefined (0x3000)".into(),
        ObjectFormatCode::Association => "Association/Folder (0x3001)".into(),
        ObjectFormatCode::Text => "Text (0x3004)".into(),
        ObjectFormatCode::Html => "HTML (0x3005)".into(),
        ObjectFormatCode::Jpeg => "JPEG (0x3801)".into(),
        ObjectFormatCode::Png => "PNG (0x380B)".into(),
        ObjectFormatCode::Gif => "GIF (0x3807)".into(),
        ObjectFormatCode::Tiff => "TIFF (0x3804)".into(),
        ObjectFormatCode::Bmp => "BMP (0x3808)".into(),
        ObjectFormatCode::Mp3 => "MP3 (0x3009)".into(),
        ObjectFormatCode::Wav => "WAV (0x3008)".into(),
        ObjectFormatCode::Avi => "AVI (0x300A)".into(),
        ObjectFormatCode::Mpeg => "MPEG (0x300B)".into(),
        ObjectFormatCode::Mp4Container => "MP4 (0xB982)".into(),
        ObjectFormatCode::M4aAudio => "M4A (0xB984)".into(),
        ObjectFormatCode::WmaAudio => "WMA (0xB901)".into(),
        ObjectFormatCode::WmvVideo => "WMV (0xB981)".into(),
        ObjectFormatCode::FlacAudio => "FLAC (0xB906)".into(),
        ObjectFormatCode::Unknown(c) => format!("Unknown(0x{c:04X})"),
        other => format!("{other:?}"),
    }
}

pub fn format_datetime(dt: &mtp_rs::DateTime) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        dt.year, dt.month, dt.day, dt.hour, dt.minute, dt.second
    )
}
