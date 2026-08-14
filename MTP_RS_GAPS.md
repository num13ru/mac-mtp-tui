# mtp-rs MTP Operation Backlog

Originally recorded against mtp-rs 0.13.2 from codes advertised by a Kindle Paperwhite.
mtp-rs 0.30.0 names the object-property operation codes below, but its portable high-level
API intentionally exposes only backend-neutral object metadata. Direct property operations
remain available only through the low-level PTP API.

This document serves as a patch backlog for mtp-rs.

## Object Property Operations

mtp-rs 0.30.0 defines these operation codes. The remaining integration question is which
operations should gain typed low-level helpers and which metadata should be portable across
the USB/PTP and Windows WPD backends.

| Hex    | 0.13.2 display  | MTP Spec Name             | Description                                                        | Priority    |
|--------|-----------------|---------------------------|--------------------------------------------------------------------|-------------|
| 0x9801 | Unknown(38913)  | `GetObjectPropsSupported` | Returns `u16[]` of supported property codes for a given format     | **HIGH**    |
| 0x9802 | Unknown(38914)  | `GetObjectPropDesc`       | Returns property descriptor (data type, default, range) per prop   | **HIGH**    |
| 0x9805 | Unknown(38917)  | `GetObjectPropList`       | Batch-fetch multiple properties in one round-trip                  | MEDIUM      |
| 0x9806 | Unknown(38918)  | `SetObjectPropList`       | Batch-set multiple property values                                 | LOW         |
| 0x9808 | Unknown(38920)  | `SendObjectPropList`      | Send property values alongside object during upload                | LOW         |
| 0x9810 | Unknown(38928)  | `GetObjectReferences`     | Get playlist/album member handles                                  | LOW         |
| 0x9811 | Unknown(38929)  | `SetObjectReferences`     | Set playlist/album member handles                                  | LOW         |
| 0x1011 | Unknown(4113)   | `SelfTest`                | PTP standard device self-test                                      | NONE        |
| 0x9202 | Unknown(37378)  | WMDRMPD vendor op         | Microsoft DRM extension (Kindle vendor ext: `microsoft.com/WMDRMPD:10.1`) | NONE |

## Missing Device Property Codes (`DevicePropertyCode` enum)

mtp-rs defines PTP camera properties (0x5000 range) but none of the MTP-specific
device properties in the 0xD400 range.

| Hex    | MTP Spec Name              | Kindle Value                              | Notes                                          |
|--------|----------------------------|-------------------------------------------|-------------------------------------------------|
| 0xD401 | `SynchronizationPartner`   | `""` (empty, RW)                          | Standard MTP property                           |
| 0xD402 | `DeviceFriendlyName`       | `"Kindle Paperwhite GN433X..."` (RO)      | Already used implicitly via DeviceInfo           |
| 0xD404 | `SupportedFormatsOrdered`  | `Uint8(1)` (RO)                           | Boolean: device returns formats in priority order|
| 0xD405 | `DeviceIcon`               | ERROR: GeneralError                       | Kindle advertises but does not actually support  |
| 0xD407 | `PerceivedDeviceType`      | `Uint32(3)` (RO)                          | 3 = "Media Player" per MTP spec                 |

## Recommended Patch Order

### 1. `GetObjectPropsSupported` (0x9801) -- Highest impact

Enables dynamic property discovery per object format. Without it, consumers must
use a fixed list of known properties and tolerate errors.

Wire format: command takes one param (format code as `u32`), data phase returns a
`u16[]` array. Straightforward to add:
- New `PtpSession::get_object_props_supported(format: ObjectFormatCode) -> Result<Vec<ObjectPropertyCode>>`

### 2. `GetObjectPropDesc` (0x9802) -- Typed property parsing

Returns a property descriptor dataset (data type, default value, form/range) for one
object property code + format code. Similar structure to the existing `DevicePropDesc`.

Would need:
- New `ObjectPropDesc` type (mirroring `DevicePropDesc`)
- New `PtpSession::get_object_prop_desc(format, prop) -> Result<ObjectPropDesc>`

### 3. MTP device property codes (0xD401-0xD407)

Low effort: add named variants to `DevicePropertyCode` enum. No new session methods needed
since `get_device_prop_desc` / `get_device_prop_value` already work with any code.

### 4. `GetObjectPropList` (0x9805) -- Performance

Batch-fetches multiple properties for one or more objects in a single MTP round-trip.
Complex wire format (MTP "ObjectPropList" dataset with TLV-style entries).
Lower priority but significant performance win for inspector-style features.

### 5. Remaining operations

`SelfTest`, `SetObjectPropList`, `SendObjectPropList`, `GetObjectReferences`,
`SetObjectReferences`, WMDRMPD -- niche use cases, unlikely to be needed soon.

## How This Affects mtp-tui

The object inspector (`i` key) uses the backend-neutral metadata returned by
`Storage::get_object_info`. It keeps a fixed list of familiar MTP property rows and marks
protocol-specific values unavailable when the high-level API does not expose them.

If mtp-rs adds portable property discovery and typed property values:

- **Patch 1** would let us query the device for supported properties first, then
  only fetch those (no more try/fail on unsupported codes).
- **Patch 2** would let us parse raw property bytes using the declared data type
  instead of hardcoding type assumptions per property code.
- **Patch 4** would let us fetch all properties in one MTP command instead of N
  sequential `GetObjectPropValue` calls.
