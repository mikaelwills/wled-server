#!/usr/bin/env python3
"""
Simple HTTP server with SPA (Single Page Application) support.
Serves index.html for all non-file requests to support client-side routing.
"""

import http.server
import socketserver
import os
import sys
from pathlib import Path

class SPAHTTPRequestHandler(http.server.SimpleHTTPRequestHandler):
    """HTTP request handler with SPA fallback support."""

    def do_GET(self):
        # Get the requested path
        path = self.translate_path(self.path)

        # If path doesn't exist and it's not a file request, serve index.html
        if not os.path.exists(path) and '.' not in os.path.basename(self.path):
            self.path = '/index.html'

        return http.server.SimpleHTTPRequestHandler.do_GET(self)

    def end_headers(self):
        # Add CORS headers if needed
        self.send_header('Access-Control-Allow-Origin', '*')
        http.server.SimpleHTTPRequestHandler.end_headers(self)

def main():
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 3011
    directory = sys.argv[2] if len(sys.argv) > 2 else '.'

    os.chdir(directory)

    with socketserver.TCPServer(("0.0.0.0", port), SPAHTTPRequestHandler) as httpd:
        print(f"Serving SPA at http://0.0.0.0:{port}")
        print(f"Directory: {os.getcwd()}")
        print("Press Ctrl+C to stop")
        try:
            httpd.serve_forever()
        except KeyboardInterrupt:
            print("\nShutting down...")
            httpd.shutdown()

if __name__ == '__main__':
    main()
