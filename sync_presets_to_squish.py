#!/usr/bin/env python3
"""
Sync all presets from backup to Squiiiiiiish board's native WLED presets.json format
"""
import json
import requests

BOARD_IP = "192.168.1.172"
BACKUP_DIR = "presets_backup"

# Load all presets from backup
import os
import glob

print("Loading presets from backup...")
presets = []
for preset_file in glob.glob(f"{BACKUP_DIR}/*.json"):
    with open(preset_file, 'r') as f:
        preset = json.load(f)
        presets.append(preset)

print(f"Loaded {len(presets)} presets")

# Fetch current WLED presets.json
print(f"\nFetching current presets from {BOARD_IP}...")
response = requests.get(f"http://{BOARD_IP}/presets.json", timeout=5)
wled_presets = response.json()

print(f"Current WLED presets count: {len([k for k in wled_presets.keys() if k != '0'])}")

# Find next available preset slot
used_slots = [int(k) for k in wled_presets.keys() if k.isdigit()]
next_slot = 1
while next_slot in used_slots:
    next_slot += 1

print(f"\nAdding presets starting from slot {next_slot}...")

# Convert our presets to WLED format and add them
for preset in presets:
    # Check if preset with same name already exists
    existing = any(p.get('n') == preset['name'] for p in wled_presets.values() if isinstance(p, dict))
    if existing:
        print(f"  Skipping '{preset['name']}' (already exists)")
        continue

    # Convert to WLED format
    wled_preset = {
        "mainseg": 0,
        "seg": [{
            "id": 0,
            "grp": 1,
            "spc": 0,
            "of": 0,
            "on": preset['state']['on'],
            "frz": False,
            "bri": preset['state']['brightness'],
            "cct": 127,
            "set": 0,
            "n": "",
            "col": [
                preset['state']['color'] + [0],  # Add white channel
                [0, 0, 0, 0],
                [0, 0, 0, 0]
            ],
            "fx": preset['state']['effect'],
            "sx": preset['state']['speed'],
            "ix": preset['state']['intensity'],
            "pal": 0,
            "c1": 128,
            "c2": 128,
            "c3": 16,
            "sel": True,
            "rev": False,
            "mi": False,
            "o1": False,
            "o2": False,
            "o3": False,
            "si": 0,
            "m12": 0
        }],
        "n": preset['name']
    }

    # Save preset to WLED board
    print(f"  Saving preset {next_slot}: {preset['name']}")

    # Use WLED API to save preset
    payload = {
        "psave": next_slot,
        "n": preset['name'],
        "seg": [{
            "bri": preset['state']['brightness'],
            "col": [preset['state']['color']],
            "fx": preset['state']['effect'],
            "sx": preset['state']['speed'],
            "ix": preset['state']['intensity']
        }]
    }

    try:
        response = requests.post(
            f"http://{BOARD_IP}/json/state",
            json=payload,
            timeout=5
        )
        if response.status_code == 200:
            print(f"    ✓ Saved to slot {next_slot}")
            next_slot += 1
        else:
            print(f"    ✗ Failed: {response.status_code}")
    except Exception as e:
        print(f"    ✗ Error: {e}")

print("\n✓ Done syncing presets to Squiiiiiiish!")
