#!/usr/bin/env python3
"""
bootstrap_platform_secret.py
────────────────────────────
Create a PLATFORM_SECRET for Apaleo (or any other platform),
retrieve the returned `_id`, and push that secret reference
(`secretsServiceId`) into the relevant `connectedPlatforms`
entry inside the Settings document.

Steps performed
---------------
1. POST /v1/secrets               → returns secret _id
2. POST /v1/settings/create       → upserts the platform block
                                    with `secretsServiceId` + masked secret
3. (optional) verify the update   → POST /v1/settings/get and print summary

Requirements
------------
pip install requests PyJWT (PyJWT only needed if you opt-in to JWT auth)

Typical usage
-------------
python add-secrets-script.py \
  --backend-url https://platform-backend.inhotel.io \
  --pica-secret  sk_test_1_3pejYG_SdSxV9xkt5_GA8WoMsSnfBHvY1qpGhlX-6DKd9kyZO3ee9hWfjGWpt5dY0AzxvM51q6_45_Q6bJTWCTuax7yq4X96nhvB0uTwhhLlsxyJm02JqasmdeDVeHt08GxGPoiBc7I9u00-1EKOejw62kNO0M1EaEFqwaGXw1Y8IfFH\
  --client-id  QWMI-AC-APALEO_PICA \
  --client-secret a0Ixq7RhlDbNJFGeJB3KLzs8CGh1tY \
  --connection-definition-id conn_def::GDYdQHhelfo::kCMpiR68QtuDvWV-dK_YEQ \
  --user-id 65648fa26b1eb500122c5323 \
  --buildable-id build-1c3cd7af757d4aebab523f5373190e1b \
  --platform apaleo \
  --environment test
"""

from __future__ import annotations
import argparse, sys, time, json
from pathlib import Path
from typing import Dict, Any

import requests


# ────────────────────────────────
# Small helpers
# ────────────────────────────────
def half_mask(value: str) -> str:
    """Return abcde*****vwxyz for nicer display in Settings.secret"""
    if len(value) <= 10:
        return "*****"
    return f"{value[:5]}*****{value[-5:]}"


# ────────────────────────────────
# CLI config
# ────────────────────────────────
def cli_config() -> argparse.Namespace:
    p = argparse.ArgumentParser(
        description="Create secret and wire it into Settings.connectedPlatforms"
    )
    p.add_argument("--backend-url", default="https://platform-backend.inhotel.io")
    p.add_argument("--pica-secret", required=True, help="X-Pica-Secret header value")

    p.add_argument("--client-id", required=True)
    p.add_argument("--client-secret", required=True)
    p.add_argument("--platform", default="apaleo")
    p.add_argument("--environment", choices=["test", "live"], default="test")

    p.add_argument(
        "--connection-definition-id",
        required=True,
        help="ConnectionDefinitionId that this platform represents",
    )
    p.add_argument("--user-id", required=True)
    p.add_argument("--buildable-id", required=True)

    p.add_argument(
        "--verify",
        action="store_true",
        help="Fetch Settings afterwards and print the updated platform block",
    )
    return p.parse_args()


# ────────────────────────────────
# Core logic
# ────────────────────────────────
class BackendClient:
    def __init__(self, cfg: argparse.Namespace):
        self.base = cfg.backend_url.rstrip("/")
        self.h = {"Content-Type": "application/json", "X-Pica-Secret": cfg.pica_secret}
        self.s = requests.Session()
        self.s.headers.update(self.h)

    # 1. create secret
    def create_secret(self, cfg: argparse.Namespace) -> str:
        url = f"{self.base}/v1/secrets"
        payload = {
            "secretType": "PLATFORM_SECRET",
            "platform": cfg.platform,
            "secret": {"CLIENT_ID": cfg.client_id, "CLIENT_SECRET": cfg.client_secret},
            "ownership": {"id": cfg.user_id, "buildableId": cfg.buildable_id},
        }
        r = self.s.post(url, json=payload, timeout=30)
        r.raise_for_status()
        sid = r.json()["_id"]
        print(f"✅ Secret created  → _id={sid}")
        return sid

    # 2. upsert platform block in settings
    def upsert_settings_platform(self, cfg: argparse.Namespace, secret_id: str):
        url = f"{self.base}/internal/v1/settings/create"  # mapped to v1.settings.public.create
        platform_block = {
            "connectionDefinitionId": cfg.connection_definition_id,
            "type": cfg.platform,
            "active": True,
            "environment": cfg.environment,
            "secretsServiceId": secret_id,
            "secret": {
                "clientId": cfg.client_id,
                "clientSecretDisplay": half_mask(cfg.client_secret),
            },
            "activatedAt": int(time.time() * 1000),
            "image": "",  # optional – leave empty or supply your own
            "title": cfg.platform.capitalize(),
        }

        payload = {"platform": platform_block}

        r = self.s.post(url, json=payload, timeout=30)
        r.raise_for_status()
        print("✅ connectedPlatform patched into Settings")

    # 3. (optional) verify
    def fetch_settings(self):
        url = f"{self.base}/v1/settings/get"
        r = self.s.post(url, json={}, timeout=15)  # alias expects POST
        r.raise_for_status()
        return r.json()


def main() -> None:
    cfg = cli_config()
    client = BackendClient(cfg)

    try:
        secret_id = client.create_secret(cfg)
        client.upsert_settings_platform(cfg, secret_id)

        if cfg.verify:
            settings = client.fetch_settings()
            matched = [
                p
                for p in settings.get("connectedPlatforms", [])
                if p.get("secretsServiceId") == secret_id
            ]
            print("\n🔎 Verification snippet:")
            print(json.dumps(matched[0] if matched else {}, indent=2))

        print("\n🎉  All done.")
    except requests.HTTPError as e:
        print(f"❌  HTTP {e.response.status_code}: {e.response.text}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
