#!/bin/bash
BACKUP_DIR="$(dirname "$0")/cue-backups"
DRY_RUN=false

if [ "$1" = "--dry-run" ]; then
    DRY_RUN=true
    shift
fi

if [ -n "$1" ]; then
    BACKUP_FILE="$1"
else
    BACKUP_FILE=$(ls -t "$BACKUP_DIR"/*.json 2>/dev/null | head -1)
fi

if [ -z "$BACKUP_FILE" ] || [ ! -f "$BACKUP_FILE" ]; then
    echo "ERROR: No backup file found"
    echo "Usage: $0 [--dry-run] [backup-file.json]"
    exit 1
fi

echo "Restoring from: $BACKUP_FILE"
[ "$DRY_RUN" = true ] && echo "(DRY RUN - no changes will be made)"
echo ""

python3 << PYEOF
import json, urllib.request, urllib.parse

dry_run = $( [ "$DRY_RUN" = true ] && echo "True" || echo "False" )

with open("$BACKUP_FILE") as f:
    backup = json.load(f)

restored = 0
skipped = 0

for prog in backup:
    pid = prog["id"]
    name = prog.get("song_name", pid)
    backup_cues = prog.get("cues", [])
    setlist = prog.get("setlist_id", "default")

    if len(backup_cues) == 0:
        print(f"  SKIP  {name:40s} (0 cues in backup)")
        skipped += 1
        continue

    if dry_run:
        encoded = urllib.parse.quote(pid, safe='')
        try:
            req = urllib.request.Request(f"http://robert:3010/api/programs/{encoded}")
            current = json.loads(urllib.request.urlopen(req).read())
            current_cues = len(current.get("cues", []))
            if current_cues == len(backup_cues):
                print(f"  OK    {name:40s} ({current_cues} cues, matches backup)")
            else:
                print(f"  WOULD {name:40s} ({current_cues} -> {len(backup_cues)} cues)")
        except:
            print(f"  NEW   {name:40s} ({len(backup_cues)} cues, not on server)")
        continue

    encoded = urllib.parse.quote(pid, safe='')
    try:
        req = urllib.request.Request(f"http://robert:3010/api/programs/{encoded}")
        current = json.loads(urllib.request.urlopen(req).read())
    except:
        print(f"  MISS  {name:40s} (not found on server, skipping)")
        skipped += 1
        continue

    current["cues"] = backup_cues
    put_data = json.dumps(current).encode()
    put_req = urllib.request.Request(
        f"http://robert:3010/api/programs/{encoded}",
        data=put_data, method="PUT",
        headers={"Content-Type": "application/json"}
    )
    urllib.request.urlopen(put_req)
    print(f"  OK    {name:40s} -> {len(backup_cues)} cues")
    restored += 1

if dry_run:
    print(f"\nDry run complete. Use without --dry-run to restore.")
else:
    print(f"\nRestored {restored} programs, skipped {skipped}")
PYEOF
