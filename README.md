# nRF BLE firmware update utility

**WORK IN PROGRESS:** DFU functionality works, but error messages are cryptic and there are no retries

Firmware update utility for BLE devices that support the
[nRF DFU](https://infocenter.nordicsemi.com/topic/sdk_nrf5_v17.1.0/lib_dfu_transport_ble.html) protocol.

An alternative to the official  [nrfutil](https://infocenter.nordicsemi.com/topic/ug_nrfutil/UG/nrfutil/nrfutil_dfu_ble.html)
which needs a special USB device connected to the host machine to run a BLE update.

## Usage

Trigger the DFU mode using the Buttonless DFU service, then update the application:
```console
nrfdfu-ble AA:BB:CC:11:22:33 trigger && \
nrfdfu-ble AA:BB:CC:11:22:34 app ./path/to/fw-pkg.zip
```
Note the change in MAC address, nRF DFU bootloader increments the last byte of the address by one.

BLE MAC addresses are not exposed on macOS, same update can be performed using device names:
```console
nrfdfu-ble "BLE Device XYZ" trigger && \
nrfdfu-ble DfuTarget app ./path/to/fw-pkg.zip
```
By default the bootloader advertises the device as `DfuTarget`.
