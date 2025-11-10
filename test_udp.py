#!/usr/bin/env python3
import socket
import struct

# Create minimal E1.31 packet for preset 5
# E1.31 packet structure (simplified):
# Root Layer, Framing Layer, DMP Layer, DMX Data

def create_e131_packet(universe, dmx_data):
    """Create a minimal E1.31 (sACN) packet"""

    # Root Layer (38 bytes)
    root_preamble_size = struct.pack('!H', 0x0010)
    root_postamble_size = struct.pack('!H', 0x0000)
    root_acn_packet_id = bytes([0x41, 0x53, 0x43, 0x2d, 0x45, 0x31, 0x2e, 0x31, 0x37, 0x00, 0x00, 0x00])  # "ASC-E1.17\0\0\0"

    # Framing layer length (88 + DMX data length)
    framing_length = 88 + len(dmx_data)
    root_flags_length = struct.pack('!H', 0x7000 | (framing_length + 38 - 16))  # 0x7000 = flags, rest is length

    root_vector = struct.pack('!I', 0x00000004)  # VECTOR_ROOT_E131_DATA
    source_cid = bytes([0x12, 0x34, 0x56, 0x78] * 4)  # 16-byte UUID

    # Framing Layer
    framing_flags_length = struct.pack('!H', 0x7000 | framing_length)
    framing_vector = struct.pack('!I', 0x00000002)  # VECTOR_E131_DATA_PACKET
    source_name = b'WLED Test'.ljust(64, b'\x00')
    priority = struct.pack('!B', 100)
    sync_address = struct.pack('!H', 0)
    sequence_number = struct.pack('!B', 0)
    options = struct.pack('!B', 0x00)  # No preview, no stream_terminated
    universe_num = struct.pack('!H', universe)

    # DMP Layer
    dmp_length = 11 + len(dmx_data)
    dmp_flags_length = struct.pack('!H', 0x7000 | dmp_length)
    dmp_vector = struct.pack('!B', 0x02)  # VECTOR_DMP_SET_PROPERTY
    dmp_address_type_data_type = struct.pack('!B', 0xa1)  # 0xa1
    dmp_first_property_address = struct.pack('!H', 0x0000)
    dmp_address_increment = struct.pack('!H', 0x0001)
    dmp_property_value_count = struct.pack('!H', len(dmx_data) + 1)
    dmx_start_code = struct.pack('!B', 0x00)

    # Assemble packet
    packet = (
        root_preamble_size +
        root_postamble_size +
        root_acn_packet_id +
        root_flags_length +
        root_vector +
        source_cid +
        framing_flags_length +
        framing_vector +
        source_name +
        priority +
        sync_address +
        sequence_number +
        options +
        universe_num +
        dmp_flags_length +
        dmp_vector +
        dmp_address_type_data_type +
        dmp_first_property_address +
        dmp_address_increment +
        dmp_property_value_count +
        dmx_start_code +
        dmx_data
    )

    return packet

# Create DMX data (512 bytes, only first byte = preset 5)
dmx_data = bytearray(512)
dmx_data[0] = 5  # Preset 5

# Create E1.31 packet
packet = create_e131_packet(universe=1, dmx_data=dmx_data)

# Send to board Two and Three
boards = [
    ("192.168.8.118", 5568),
    ("192.168.8.210", 5568),
]

sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)

for ip, port in boards:
    print(f"Sending E1.31 packet to {ip}:{port} (preset 5)...")
    sock.sendto(packet, (ip, port))
    print(f"  Sent {len(packet)} bytes")

sock.close()
print("Done!")
