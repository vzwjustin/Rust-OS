#!/usr/bin/python3
# SPDX-FileCopyrightText: 2021 Dylan Van Assche <me@dylanvanassche.be>
# SPDX-License-Identifier: GPL-3.0-or-later
import sys
from os.path import realpath, join, dirname

# Add topdir to import path
topdir = realpath(join(dirname(__file__) + "/.."))  # noqa
sys.path.insert(0, topdir)  # noqa

from argparse import ArgumentParser, SUPPRESS
from logging import debug, basicConfig, DEBUG, WARNING
from xml.etree import ElementTree
from typing import Tuple, Optional, cast
from os import environ
from os.path import abspath

from obex_capabilities.device import Device, guess_device
from obex_capabilities.modem import Modem, guess_modem

VERSION = '0.1.1'
XML_TEMPLATE = 'data/template.xml'
XML_TEMPLATE = """<?xml version="1.0"?>
<!DOCTYPE Capability SYSTEM "obex-capability.dtd">
<Capability Version="1.0">
 <General>
  <Manufacturer></Manufacturer>
  <Model></Model>
  <SN></SN>
  <SW version=""/>
  <OS version="" id=""/>
 </General>
 <Service>
  <UUID>SYNCML-SYNC</UUID>
  <Name>SyncML</Name>
  <Version>1.2</Version>
  <Object>
   <Type>application/vnd.syncml+wbxml</Type>
   <Ext>
    <XVal>application/vnd.syncml.ds.notification</XVal>
    <XNam>ServerAlertedNotificationType</XNam>
   </Ext>
  </Object>
 </Service>
</Capability>"""


def generate_capabilities(device: Device, modem: Optional[Modem]):
    tree = ElementTree.ElementTree(ElementTree.fromstring(XML_TEMPLATE))
    root = tree.getroot()

    debug('Generating capabilities')

    # Device and OS information
    manufacturer = root.findall('./General/Manufacturer')[0]
    model = root.findall('./General/Model')[0]
    unique_id = root.findall('./General/SN')[0]
    software = root.findall('./General/SW')[0]
    os = root.findall('./General/OS')[0]
    manufacturer.text = device.manufacturer
    model.text = device.model
    unique_id.text = device.unique_id
    software.set('version', device.software_version)
    os.set('version', device.os_version)
    os.set('id', device.codename)

    # Modem information
    if modem is not None:
        general = root.findall('./General')[0]
        ext = ElementTree.SubElement(general, 'Ext')
        xnam = ElementTree.SubElement(ext, 'XNam')
        xnam.text = 'NetworkInfo'
        current_network = ElementTree.SubElement(ext, 'XVal')
        current_network.text = f'CurrentNetwork={modem.network}'
        country_code = ElementTree.SubElement(ext, 'XVal')
        country_code.text = f'CountryCode={modem.mcc}'
        modem_id = ElementTree.SubElement(ext, 'XVal')
        modem_id.text = f'NetworkID={modem.mnc}'

    debug('Generation complete, serializing XML')

    # Pretty print to stdout for obexd
    ElementTree.indent(tree, space=' ')
    capabilities = ElementTree.tostring(root).decode()
    print(capabilities)


def main():
    # Parse arguments
    parser = ArgumentParser(description='Generator tool for OBEX capabilities')
    parser.add_argument('--debug', help='Enable logging to stderr',
                        dest='debug', action='store_true')
    parser.set_defaults(debug=False, test=False)
    args = parser.parse_args()

    # Configure logging
    lvl = WARNING
    if args.debug:
        lvl = DEBUG
    basicConfig(level=lvl)

    modem: Modem = guess_modem()
    device: Device = guess_device(modem)

    # Generate capabilities
    generate_capabilities(device, modem)


if __name__ == '__main__':
    main()
