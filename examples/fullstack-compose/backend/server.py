#!/usr/bin/env python3
import json
import os
import sys
from http.server import HTTPServer, BaseHTTPRequestHandler

DATA_DIR = "/data"
DATA_FILE = os.path.join(DATA_DIR, "items.json")

class ApiHandler(BaseHTTPRequestHandler):
    def _set_headers(self, status=200, content_type="application/json"):
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        self.end_headers()

    def do_OPTIONS(self):
        self._set_headers(204)

    def do_GET(self):
        if self.path == "/" or self.path == "/api":
            self._set_headers()
            response = {
                "service": "boxr-backend-api",
                "status": "online",
                "env": os.environ.get("APP_ENV", "development"),
                "cache_host": os.environ.get("CACHE_HOST", "localhost"),
                "engine": "boxr OCI runtime",
            }
            self.wfile.write(json.dumps(response).encode("utf-8"))
        elif self.path == "/health":
            self._set_headers()
            response = {
                "status": "healthy",
                "pid": os.getpid(),
                "uptime": "ok",
            }
            self.wfile.write(json.dumps(response).encode("utf-8"))
        elif self.path == "/items":
            self._set_headers()
            items = self.read_items()
            self.wfile.write(json.dumps({"count": len(items), "items": items}).encode("utf-8"))
        else:
            self._set_headers(404)
            self.wfile.write(json.dumps({"error": "Not Found"}).encode("utf-8"))

    def do_POST(self):
        if self.path == "/items":
            content_length = int(self.headers.get("Content-Length", 0))
            post_data = self.rfile.read(content_length)
            try:
                new_item = json.loads(post_data.decode("utf-8"))
            except Exception:
                new_item = {"payload": post_data.decode("utf-8", errors="ignore")}

            items = self.read_items()
            items.append(new_item)
            self.save_items(items)

            self._set_headers(201)
            self.wfile.write(json.dumps({"message": "Item stored in persistent volume", "item": new_item}).encode("utf-8"))
        else:
            self._set_headers(404)
            self.wfile.write(json.dumps({"error": "Not Found"}).encode("utf-8"))

    def read_items(self):
        os.makedirs(DATA_DIR, exist_ok=True)
        if os.path.exists(DATA_FILE):
            try:
                with open(DATA_FILE, "r") as f:
                    return json.load(f)
            except Exception:
                return []
        return ["Initial Boxr Item #1", "Persistent Volume Item #2"]

    def save_items(self, items):
        os.makedirs(DATA_DIR, exist_ok=True)
        with open(DATA_FILE, "w") as f:
            json.dump(items, f, indent=2)

def run(port=5000):
    server_address = ("0.0.0.0", port)
    httpd = HTTPServer(server_address, ApiHandler)
    print(f"Backend API server listening on 0.0.0.0:{port}...")
    sys.stdout.flush()
    httpd.serve_forever()

if __name__ == "__main__":
    run(5000)
