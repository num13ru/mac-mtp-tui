use mtp_rs::transport::{NusbTransport, Transport};
use mtp_rs::ResponseCode;
use std::sync::Arc;

fn main() {
    println!("=== Transaction ID test ===\n");

    let rt = tokio::runtime::Runtime::new().unwrap();
    let devices = NusbTransport::list_mtp_devices().expect("list failed");
    if devices.is_empty() {
        println!("No MTP devices found.");
        return;
    }

    let nusb_device = devices[0].open().expect("open failed");
    let transport: Arc<dyn Transport> = Arc::new(
        rt.block_on(NusbTransport::open(nusb_device)).expect("transport failed"),
    );

    // Test A: tx_id=1 (what mtp-rs does) - expected to FAIL
    println!("--- OpenSession with tx_id=1, session_id=1 (mtp-rs default) ---");
    let cmd_a: Vec<u8> = vec![
        0x10, 0x00, 0x00, 0x00,
        0x01, 0x00,
        0x02, 0x10,
        0x01, 0x00, 0x00, 0x00, // tx_id = 1
        0x01, 0x00, 0x00, 0x00, // session_id = 1
    ];
    send_and_print(&rt, &transport, &cmd_a);

    // Test B: tx_id=0 (what go-mtpfs does) - expected to SUCCEED
    println!("\n--- OpenSession with tx_id=0, session_id=1 (go-mtpfs style) ---");
    let cmd_b: Vec<u8> = vec![
        0x10, 0x00, 0x00, 0x00,
        0x01, 0x00,
        0x02, 0x10,
        0x00, 0x00, 0x00, 0x00, // tx_id = 0 (SESSION_LESS)
        0x01, 0x00, 0x00, 0x00, // session_id = 1
    ];
    send_and_print(&rt, &transport, &cmd_b);

    // If B succeeded, try GetStorageIDs
    println!("\n--- GetStorageIDs (tx_id=1) ---");
    let cmd_c: Vec<u8> = vec![
        0x0c, 0x00, 0x00, 0x00,
        0x01, 0x00,
        0x04, 0x10,
        0x01, 0x00, 0x00, 0x00, // tx_id = 1
    ];
    send_and_print(&rt, &transport, &cmd_c);
}

fn send_and_print(rt: &tokio::runtime::Runtime, transport: &Arc<dyn Transport>, data: &[u8]) {
    let hex: Vec<String> = data.iter().map(|b| format!("{b:02x}")).collect();
    println!("  send: [{}]", hex.join(" "));

    if let Err(e) = rt.block_on(transport.send_bulk(data)) {
        println!("  SEND FAILED: {e}");
        return;
    }

    match rt.block_on(transport.receive_bulk(65536)) {
        Ok(resp) => {
            let hex: Vec<String> = resp.iter().take(40).map(|b| format!("{b:02x}")).collect();
            println!("  recv: [{}]", hex.join(" "));
            if resp.len() >= 8 {
                let pkt_type = u16::from_le_bytes([resp[4], resp[5]]);
                let pkt_code = u16::from_le_bytes([resp[6], resp[7]]);
                if pkt_type == 3 {
                    println!("  => {:?} (0x{pkt_code:04x})", ResponseCode::from(pkt_code));
                } else if pkt_type == 2 {
                    println!("  => DATA ({} bytes)", resp.len());
                    if let Ok(r2) = rt.block_on(transport.receive_bulk(512)) {
                        let h2: Vec<String> = r2.iter().map(|b| format!("{b:02x}")).collect();
                        println!("  recv: [{}]", h2.join(" "));
                        if r2.len() >= 8 {
                            let rc = u16::from_le_bytes([r2[6], r2[7]]);
                            println!("  => {:?} (0x{rc:04x})", ResponseCode::from(rc));
                        }
                    }
                }
            }
        }
        Err(e) => println!("  RECV FAILED: {e}"),
    }
}
