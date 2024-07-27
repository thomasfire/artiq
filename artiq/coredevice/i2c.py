"""
Non-realtime drivers for I2C chips on the core device.
"""

from numpy import int32

from artiq.language.core import nac3, extern, kernel, KernelInvariant
from artiq.coredevice.exceptions import I2CError
from artiq.coredevice.core import Core


@extern
def i2c_start(busno: int32):
    raise NotImplementedError("syscall not simulated")


@extern
def i2c_restart(busno: int32):
    raise NotImplementedError("syscall not simulated")


@extern
def i2c_stop(busno: int32):
    raise NotImplementedError("syscall not simulated")


@extern
def i2c_write(busno: int32, b: int32) -> bool:
    raise NotImplementedError("syscall not simulated")


@extern
def i2c_read(busno: int32, ack: bool) -> int32:
    raise NotImplementedError("syscall not simulated")


@extern
def i2c_switch_select(busno: int32, address: int32, mask: int32):
    raise NotImplementedError("syscall not simulated")


@kernel
def i2c_poll(busno: int32, busaddr: int32) -> bool:
    """Poll I2C device at address.

    :param busno: I2C bus number
    :param busaddr: 8-bit I2C device address (LSB=0)
    :returns: True if the poll was ACKed
    """
    i2c_start(busno)
    ack = i2c_write(busno, busaddr)
    i2c_stop(busno)
    return ack


@kernel
def i2c_write_byte(busno: int32, busaddr: int32, data: int32, ack: bool = True):
    """Write one byte to a device.

    :param busno: I2C bus number
    :param busaddr: 8-bit I2C device address (LSB=0)
    :param data: Data byte to be written
    :param nack: Allow NACK
    """
    i2c_start(busno)
    try:
        if not i2c_write(busno, busaddr):
            raise I2CError("failed to ack bus address")
        if not i2c_write(busno, data) and ack:
            raise I2CError("failed to ack write data")
    finally:
        i2c_stop(busno)


@kernel
def i2c_read_byte(busno: int32, busaddr: int32) -> int32:
    """Read one byte from a device.

    :param busno: I2C bus number
    :param busaddr: 8-bit I2C device address (LSB=0)
    :returns: Byte read
    """
    i2c_start(busno)
    data = 0
    try:
        if not i2c_write(busno, busaddr | 1):
            raise I2CError("failed to ack bus read address")
        data = i2c_read(busno, ack=False)
    finally:
        i2c_stop(busno)
    return data


@kernel
def i2c_write_many(busno: int32, busaddr: int32, addr: int32, data: list[int32], ack_last: bool = True):
    """Transfer multiple bytes to a device.

    :param busno: I2c bus number
    :param busaddr: 8-bit I2C device address (LSB=0)
    :param addr: 8-bit data address
    :param data: Data bytes to be written
    :param ack_last: Expect I2C ACK of the last byte written. If ``False``,
        the last byte may be NACKed (e.g. EEPROM full page writes).
    """
    n = len(data)
    i2c_start(busno)
    try:
        if not i2c_write(busno, busaddr):
            raise I2CError("failed to ack bus address")
        if not i2c_write(busno, addr):
            raise I2CError("failed to ack data address")
        for i in range(n):
            if not i2c_write(busno, data[i]) and (
                    i < n - 1 or ack_last):
                raise I2CError("failed to ack write data")
    finally:
        i2c_stop(busno)


@kernel
def i2c_read_many(busno: int32, busaddr: int32, addr: int32, data: list[int32]):
    """Transfer multiple bytes from a device.

    :param busno: I2c bus number
    :param busaddr: 8-bit I2C device address (LSB=0)
    :param addr: 8-bit data address
    :param data: List of integers to be filled with the data read.
        One entry ber byte.
    """
    m = len(data)
    i2c_start(busno)
    try:
        if not i2c_write(busno, busaddr):
            raise I2CError("failed to ack bus address")
        if not i2c_write(busno, addr):
            raise I2CError("failed to ack data address")
        i2c_restart(busno)
        if not i2c_write(busno, busaddr | 1):
            raise I2CError("failed to ack bus read address")
        for i in range(m):
            data[i] = i2c_read(busno, ack=i < m - 1)
    finally:
        i2c_stop(busno)


@nac3
class I2CSwitch:
    """Driver for the I2C bus switch.

    PCA954X (or other) type detection is done by the CPU during I2C init.

    I2C transactions are not real-time, and are performed by the CPU without
    involving RTIO.

    On the KC705, this chip is used for selecting the I2C buses on the two FMC
    connectors. HPC=1, LPC=2.
    """

    core: KernelInvariant[Core]
    busno: KernelInvariant[int32]
    address: KernelInvariant[int32]

    def __init__(self, dmgr, busno=0, address=0xe8, core_device="core"):
        self.core = dmgr.get(core_device)
        self.busno = busno
        self.address = address

    @kernel
    def set(self, channel: int32):
        """Enable one channel.

        :param channel: channel number (0-7)
        """
        i2c_switch_select(self.busno, self.address >> 1, 1 << channel)

    @kernel
    def unset(self):
        """Disable output of the I2C switch.
        """
        i2c_switch_select(self.busno, self.address >> 1, 0)


@kernel
class TCA6424A:
    """Driver for the TCA6424A I2C I/O expander.

    I2C transactions are not real-time, and are performed by the CPU without
    involving RTIO.

    On the NIST QC2 hardware, this chip is used for switching the directions
    of TTL buffers."""

    core: KernelInvariant[Core]
    busno: KernelInvariant[int32]
    address: KernelInvariant[int32]

    def __init__(self, dmgr, busno=0, address=0x44, core_device="core"):
        self.core = dmgr.get(core_device)
        self.busno = busno
        self.address = address

    @kernel
    def _write24(self, addr: int32, value: int32):
        i2c_write_many(self.busno, self.address, addr,
                       [value >> 16, value >> 8, value])

    @kernel
    def set(self, outputs: int32):
        """Drive all pins of the chip to the levels given by the
        specified 24-bit word.

        On the QC2 hardware, the LSB of the word determines the direction of
        TTL0 (on a given FMC card) and the MSB that of TTL23.

        A bit set to 1 means the TTL is an output.
        """
        outputs_le = (
            ((outputs & 0xff0000) >> 16) |
            (outputs & 0x00ff00) |
            (outputs & 0x0000ff) << 16)

        self._write24(0x8c, 0)  # set all directions to output
        self._write24(0x84, outputs_le)  # set levels


@nac3
class PCF8574A:
    """Driver for the PCF8574 I2C remote 8-bit I/O expander.

    I2C transactions are not real-time, and are performed by the CPU without
    involving RTIO.
    """

    core: KernelInvariant[Core]
    busno: KernelInvariant[int32]
    address: KernelInvariant[int32]

    def __init__(self, dmgr, busno=0, address=0x7c, core_device="core"):
        self.core = dmgr.get(core_device)
        self.busno = busno
        self.address = address

    @kernel
    def set(self, data: int32):
        """Drive data on the quasi-bidirectional pins.

        :param data: Pin data. High bits are weakly driven high
            (and thus inputs), low bits are strongly driven low.
        """
        i2c_start(self.busno)
        try:
            if not i2c_write(self.busno, self.address):
                raise I2CError("PCF8574A failed to ack address")
            if not i2c_write(self.busno, data):
                raise I2CError("PCF8574A failed to ack data")
        finally:
            i2c_stop(self.busno)

    @kernel
    def get(self) -> int32:
        """Retrieve quasi-bidirectional pin input data.

        :return: Pin data
        """
        i2c_start(self.busno)
        ret = 0
        try:
            if not i2c_write(self.busno, self.address | 1):
                raise I2CError("PCF8574A failed to ack address")
            ret = i2c_read(self.busno, False)
        finally:
            i2c_stop(self.busno)
        return ret
