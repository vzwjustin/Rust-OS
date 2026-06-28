# SPDX-FileCopyrightText: 2021 Dylan Van Assche <me@dylanvanassche.be>
# SPDX-License-Identifier: GPL-3.0-or-later
from os.path import exists
from os import environ
from abc import ABC, abstractmethod
from logging import debug, critical
from typing import Optional, cast
from .modem import Modem

OS_RELEASE_PATH = '/etc/os-release'
MACHINE_ID_PATH = '/etc/machine-id'
DMI_MANUFACTURER = '/sys/devices/virtual/dmi/id/sys_vendor'
DMI_MODEL = '/sys/devices/virtual/dmi/id/product_name'
DMI_CODENAME = '/sys/devices/virtual/dmi/id/product_name'
DEVICE_TREE_COMPATIBLE = '/proc/device-tree/compatible'
DEVICE_TREE_MODEL = '/proc/device-tree/model'


class Device(ABC):
    """
    Represent the information about a device.
    """

    def __init__(self, modem=None):
        self._os_release: str = None
        self._unique_id: str = None
        self._modem: Modem = modem
        self._os_release_path = environ.get('OS_RELEASE_PATH', OS_RELEASE_PATH)
        self._machine_id_path = environ.get('MACHINE_ID_PATH', MACHINE_ID_PATH)

    def __repr__(self):
        return 'DEVICE' \
            f'\n  Manufacturer: {self.manufacturer}' \
            f'\n  Model: {self.model}' \
            f'\n  Codename: {self.codename}' \
            f'\n  Unique ID: {self.unique_id}' \
            f'\n  Software version: {self.software_version}' \
            f'\n  OS version: {self.os_version}'

    def __str__(self):
        return self.__repr__()

    def _read_os_release(self) -> str:
        """
        Read /etc/os-release to determine OS version
        """
        # Determine OS release only once
        if self._os_release is not None:
            return self._os_release

        with open(self._os_release_path) as f:
            for line in filter(lambda line: '=' in line, f.readlines()):
                key, value = line.split('=')
                if key.lower() == 'version_id':
                    self._os_release = value.strip().replace('"', '')
                    break

        return self._os_release

    @property
    @abstractmethod
    def manufacturer(self) -> str:
        """
        Pretty name of the manufacturer.
        """

    @property
    @abstractmethod
    def model(self) -> str:
        """
        Pretty name of the device model.
        """

    @property
    @abstractmethod
    def codename(self) -> str:
        """
        Code name of the device.
        """

    @property
    def unique_id(self) -> str:
        """
        Unique identifier such as IMEI or MAC address.
        """
        # Determine unique ID only once
        if self._unique_id is not None:
            return self._unique_id

        if self._modem is not None:
            debug('Found modem, using IMEI')
            self._unique_id = cast(str, self._modem.imei)
        else:
            debug('No modem available, using machine-id')
            with open(self._machine_id_path) as f:
                self._unique_id = f.read().strip()

        return self._unique_id

    @property
    def software_version(self) -> str:
        """
        Software version release number.
        """
        return self._read_os_release()

    @property
    def os_version(self) -> str:
        """
        OS version release number.
        """
        return self._read_os_release()


class ARMDevice(Device):
    def __init__(self, modem=None):
        super().__init__(modem)

    @property
    def manufacturer(self) -> str:
        with open(DEVICE_TREE_COMPATIBLE) as f:
            compatible = f.read().split('\x00')
            manufacturer, _ = compatible[0].split(',')
            return manufacturer

    @property
    def model(self) -> str:
        with open(DEVICE_TREE_MODEL) as f:
            model = f.read().split('\x00')[0]
            return model

    @property
    def codename(self) -> str:
        with open(DEVICE_TREE_COMPATIBLE) as f:
            compatible = f.read().split('\x00')
            _, codename = compatible[0].split(',')
            return codename


class x86Device(Device):
    def __init__(self, modem=None):
        super().__init__(modem)

    @property
    def manufacturer(self) -> str:
        with open(DMI_MANUFACTURER) as f:
            return f.read().strip()

    @property
    def model(self) -> str:
        with open(DMI_MODEL) as f:
            return f.read().strip()

    @property
    def codename(self) -> str:
        with open(DMI_CODENAME) as f:
            return f.read().strip()


class MockedDevice(Device):
    def __init__(self, modem=None):
        super().__init__(modem)

    @property
    def manufacturer(self) -> str:
        return 'Manufacturer'

    @property
    def model(self) -> str:
        return 'Phone'

    @property
    def codename(self) -> str:
        return 'my-awesome-codename'

    @property
    def unique_id(self) -> str:
        return '123456789012345'

    @property
    def software_version(self) -> str:
        return '1.0.0'

    @property
    def os_version(self) -> str:
        return '2.0.0'


def guess_device(modem=None):
    if environ.get('MOCK_DEVICE', False):
        return MockedDevice(modem)
    elif exists(DMI_MODEL):
        debug("Device is x86")
        return x86Device(modem)
    elif exists(DEVICE_TREE_COMPATIBLE):
        debug("Device is ARM")
        return ARMDevice(modem)
    else:
        critical("Device not implemented! Falling back to MockedDevice")
        return MockedDevice(modem)
