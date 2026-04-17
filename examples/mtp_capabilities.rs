//! PTP/MTP device diagnostic tool.
//!
//! Dumps the full DeviceInfo (operations, events, properties, formats),
//! enumerates storages, and lists root objects as a smoke test.
//! All codes are printed with raw hex values so vendor-specific extensions
//! are immediately visible.
//!
//! Run with: cargo run --example mtp_capabilities

use mtp_rs::ptp::{DevicePropertyCode, OperationCode, PtpDevice, StorageId};

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== PTP/MTP Device Diagnostic ===\n");

    let device = PtpDevice::open_first().await?;
    let session = device.open_session().await?;

    // ── DeviceInfo ──────────────────────────────────────────────────────
    let info = session.get_device_info().await?;

    println!("--- Device Identity ---");
    println!("  Manufacturer : {}", info.manufacturer);
    println!("  Model        : {}", info.model);
    println!("  Device ver.  : {}", info.device_version);
    println!("  Serial       : {}", info.serial_number);
    println!("  PTP version  : {}.{:02}", info.standard_version / 100, info.standard_version % 100);
    println!("  Vendor ext ID: 0x{:08X}", info.vendor_extension_id);
    println!("  Vendor ext v.: {}", info.vendor_extension_version);
    println!("  Vendor ext   : {:?}", info.vendor_extension_desc);
    println!("  Functional   : {}", info.functional_mode);
    println!();

    // ── Supported Operations ────────────────────────────────────────────
    println!("--- Supported Operations ({}) ---", info.operations_supported.len());
    for op in &info.operations_supported {
        let raw: u16 = (*op).into();
        println!("  0x{raw:04X}  {op:?}");
    }
    println!();

    // ── Supported Events ────────────────────────────────────────────────
    println!("--- Supported Events ({}) ---", info.events_supported.len());
    for ev in &info.events_supported {
        let raw: u16 = (*ev).into();
        println!("  0x{raw:04X}  {ev:?}");
    }
    println!();

    // ── Device Properties ───────────────────────────────────────────────
    println!("--- Device Properties ({}) ---", info.device_properties_supported.len());
    for &prop_code in &info.device_properties_supported {
        let prop = DevicePropertyCode::from(prop_code);
        print!("  0x{prop_code:04X}  {prop:?}");

        match session.get_device_prop_desc(prop).await {
            Ok(desc) => {
                let rw = if desc.writable { "RW" } else { "RO" };
                println!("  [{rw}] = {:?}", desc.current_value);
            }
            Err(e) => println!("  (error: {e})"),
        }
    }
    println!();

    // ── Capture Formats ─────────────────────────────────────────────────
    println!("--- Capture Formats ({}) ---", info.capture_formats.len());
    for fmt in &info.capture_formats {
        let raw: u16 = (*fmt).into();
        println!("  0x{raw:04X}  {fmt:?}");
    }
    println!();

    // ── Playback Formats ────────────────────────────────────────────────
    println!("--- Playback Formats ({}) ---", info.playback_formats.len());
    for fmt in &info.playback_formats {
        let raw: u16 = (*fmt).into();
        println!("  0x{raw:04X}  {fmt:?}");
    }
    println!();

    // ── Capability Summary ──────────────────────────────────────────────
    println!("--- Capability Quick-Check ---");
    let checks: &[(OperationCode, &str)] = &[
        (OperationCode::GetStorageIds,    "GetStorageIds"),
        (OperationCode::GetStorageInfo,   "GetStorageInfo"),
        (OperationCode::GetObjectHandles, "GetObjectHandles"),
        (OperationCode::GetObjectInfo,    "GetObjectInfo"),
        (OperationCode::GetObject,        "GetObject (download)"),
        (OperationCode::GetPartialObject, "GetPartialObject (range download)"),
        (OperationCode::SendObjectInfo,   "SendObjectInfo (upload prep)"),
        (OperationCode::SendObject,       "SendObject (upload data)"),
        (OperationCode::DeleteObject,     "DeleteObject"),
        (OperationCode::MoveObject,       "MoveObject"),
        (OperationCode::CopyObject,       "CopyObject"),
        (OperationCode::GetObjectPropValue, "GetObjectPropValue (MTP)"),
        (OperationCode::SetObjectPropValue, "SetObjectPropValue (MTP rename)"),
        (OperationCode::GetDevicePropDesc,  "GetDevicePropDesc"),
        (OperationCode::GetDevicePropValue, "GetDevicePropValue"),
        (OperationCode::SetDevicePropValue, "SetDevicePropValue"),
        (OperationCode::InitiateCapture,    "InitiateCapture (camera)"),
    ];
    for (op, label) in checks {
        let mark = if info.supports_operation(*op) { "+" } else { "-" };
        println!("  [{mark}] {label}");
    }
    println!();

    // ── Storages ────────────────────────────────────────────────────────
    println!("--- Storages ---");
    let storage_ids = session.get_storage_ids().await?;
    println!("  Found {} storage(s)", storage_ids.len());
    println!();

    for sid in &storage_ids {
        println!("  Storage 0x{:08X}", sid.0);
        match session.get_storage_info(*sid).await {
            Ok(si) => {
                println!("    Type       : {:?}", si.storage_type);
                println!("    Filesystem : {:?}", si.filesystem_type);
                println!("    Access     : {:?}", si.access_capability);
                println!("    Capacity   : {}", format_bytes(si.max_capacity));
                println!("    Free       : {}", format_bytes(si.free_space_bytes));
                if si.free_space_objects != 0xFFFFFFFF {
                    println!("    Free objs  : {}", si.free_space_objects);
                }
                if !si.description.is_empty() {
                    println!("    Description: {}", si.description);
                }
                if !si.volume_identifier.is_empty() {
                    println!("    Volume ID  : {}", si.volume_identifier);
                }
            }
            Err(e) => println!("    Error: {e}"),
        }
        println!();
    }

    // ── Root Object Listing (smoke test) ────────────────────────────────
    println!("--- Root Objects (first storage) ---");
    let target_storage = storage_ids.first().copied().unwrap_or(StorageId::ALL);
    match session.get_object_handles(target_storage, None, None).await {
        Ok(handles) => {
            println!("  {} object(s) in root", handles.len());
            let limit = 20;
            for handle in handles.iter().take(limit) {
                match session.get_object_info(*handle).await {
                    Ok(oi) => {
                        let kind = if oi.is_folder() { "DIR " } else { "FILE" };
                        if oi.is_folder() {
                            println!("  {kind}  {}", oi.filename);
                        } else {
                            println!("  {kind}  {} ({})", oi.filename, format_bytes(oi.size as u64));
                        }
                    }
                    Err(e) => println!("  handle {:?}: error {e}", handle),
                }
            }
            if handles.len() > limit {
                println!("  ... and {} more", handles.len() - limit);
            }
        }
        Err(e) => println!("  Error listing root: {e}"),
    }

    println!("\n=== Diagnostic complete ===");
    Ok(())
}
