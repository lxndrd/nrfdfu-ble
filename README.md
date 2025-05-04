# Cross-platform nRF BLE firmware update utility

Firmware update utility for BLE devices that support the [nRF DFU][1] protocol.

An alternative to the official [nrfutil][2], which requires an nRF5 devkit
connected to the host machine in order to perform a BLE update.

[1]: https://infocenter.nordicsemi.com/topic/sdk_nrf5_v17.1.0/lib_dfu_transport_ble.html
[2]: https://docs.nordicsemi.com/bundle/nrfutil/page/nrfutil-nrf5sdk-tools/guides/dfu_performing.html#dfu-over-bluetooth-low-energy

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

It is also possible to update the bootloader, the softdevice, or both, with a combined image:
```console
nrfdfu-ble DfuTarget sd ./path/to/fw-pkg.zip
nrfdfu-ble DfuTarget bl ./path/to/fw-pkg.zip
nrfdfu-ble DfuTarget sdbl ./path/to/fw-pkg.zip
```
