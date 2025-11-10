#!/bin/bash

# Test Loopy Pro OSC connection
echo "Testing Loopy Pro OSC connection..."
echo ""

# 1. Update Loopy Pro settings
echo "1. Updating Loopy Pro IP to 192.168.8.136:9595..."
curl -s -X PUT http://localhost:3010/api/settings/loopy-pro \
  -H "Content-Type: application/json" \
  -d '{"ip":"192.168.8.136","port":9595}'
echo ""
echo ""

# 2. Verify settings were saved
echo "2. Verifying settings..."
curl -s http://localhost:3010/api/settings/loopy-pro | python3 -m json.tool
echo ""
echo ""

# 3. Send test OSC message to Loopy Pro track 0:1
echo "3. Sending test OSC message to /PlayStop/0:1..."
curl -s -X POST http://localhost:3010/api/osc \
  -H "Content-Type: application/json" \
  -d '{"address":"/PlayStop/0:1","ip":"192.168.8.136","port":9595}'
echo ""
echo ""

echo "✓ Test complete!"
echo "If Loopy Pro is running with OSC enabled on port 9595,"
echo "track 1 on page 0 should have toggled play/stop."
