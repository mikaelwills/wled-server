#!/bin/bash
BACKUP_DIR="$(dirname "$0")/cue-backups"
mkdir -p "$BACKUP_DIR"

TIMESTAMP=$(date +%Y%m%d-%H%M%S)
BACKUP_FILE="$BACKUP_DIR/$TIMESTAMP.json"

echo "Backing up programs from robert:3010..."
curl -s "http://robert:3010/api/programs" > "$BACKUP_FILE"

if [ ! -s "$BACKUP_FILE" ]; then
    echo "ERROR: No data received from server"
    rm -f "$BACKUP_FILE"
    exit 1
fi

python3 -c "
import json
with open('$BACKUP_FILE') as f:
    programs = json.load(f)
total_cues = 0
for p in programs:
    cues = len(p.get('cues', []))
    total_cues += cues
    print(f'  {cues:3d} cues | {p.get(\"setlist_id\",\"default\"):25s} | {p[\"song_name\"]}')
print(f'\nBacked up {len(programs)} programs ({total_cues} total cues)')
"

echo "Saved to: $BACKUP_FILE"
