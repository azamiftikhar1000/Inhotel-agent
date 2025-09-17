#!/usr/bin/env python3
"""insert_model_defs.py 

Convert an OpenAPI-v2 (Swagger) JSON spec into the JSON payload(s) required by
Inhotel-OS `/v1/connection-model-definitions` and optionally POST them via
`curl`, now enriched with endpoint documentation under `knowledge`, storing full response definitions, and including required `samples` field.
Supports processing either a single spec file or an entire directory of .json specs.

Usage examples:
  # Process a single spec file and print payloads
  python insert_model_defs.py \
    --spec path/to/availability.json \
    --platform apaleo \
    --definition-id conn_def::XYZ123 \
    --base-url https://api.apaleo.com
    --bearer $JWT_TOKEN
    --post

  # Process a directory of specs, write outputs to `out/` directory
  python insert_model_defs.py \
    --spec specs_dir/ \
    --platform apaleo \
    --definition-id conn_def::XYZ123 \
    --base-url https://api.apaleo.com \
    --bearer $JWT_TOKEN \
    --output out/

  # POST directly to your local Inhotel backend
  python insert_model_defs.py \
    --spec specs_dir/ \
    --platform apaleo \
    --definition-id conn_def::XYZ123 \
    --base-url https://api.apaleo.com \
    --bearer $JWT_TOKEN \
    --output out/ \
    --post \
    --target http://localhost:3005/v1/connection-model-definitions

  # Filter endpoints by required scopes
  python insert_model_defs.py \
    --spec specs_dir/ \
    --platform apaleo \
    --definition-id conn_def::XYZ123 \
    --base-url https://api.apaleo.com \
    --scopes setup.read properties.read \
    --output out/

  # Filter endpoints by explicit endpoint list
  python insert_model_defs.py \
    --spec specs_dir/ \
    --platform apaleo \
    --definition-id conn_def::XYZ123 \
    --base-url https://api.apaleo.com \
    --endpoint-list '[{"path": "/booking/v1/blocks", "method": "get"}]' \
    --output out/

  # Filter endpoints by endpoint list from file
  python insert_model_defs.py \
    --spec specs_dir/ \
    --platform apaleo \
    --definition-id conn_def::XYZ123 \
    --base-url https://api.apaleo.com \
    --endpoint-list endpoints.json \
    --output out/
    --post \
    --target https://platform-backend.inhotel.io/v1/connection-model-definitions
"""

import argparse
import datetime as _dt
import json
import os
import re
import subprocess
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

# ---------------------------------------------------------------------------
# Helper utilities
# ---------------------------------------------------------------------------

def camel_case(s: str) -> str:
    parts = [p for p in re.split(r"[^A-Za-z0-9]", s) if p]
    return "".join(part.capitalize() for part in parts)

def now_ms() -> str:
    return str(int(_dt.datetime.utcnow().timestamp() * 1000))

def first_non_template_segment(path: str) -> str:
    for segment in path.split("/"):
        if segment and not segment.startswith("{"):
            return segment
    return "root"

ALLOWED_ACTIONS = {"create", "update", "getMany", "getOne", "getCount", "delete"}
HTTP_TO_ACTION = {"post": "create", "patch": "update", "delete": "delete"}

# ---------------------------------------------------------------------------
# Filtering utilities
# ---------------------------------------------------------------------------

def has_required_scopes(op: Dict[str, Any], required_scopes: List[str]) -> bool:
    """Check if the operation documentation contains all the required scopes."""
    if not required_scopes:
        return True
    
    # Extract documentation from operation
    docs = ""
    if "summary" in op:
        docs += op["summary"] + " "
    if "description" in op:
        docs += op["description"] + " "
    
    # Check for security requirements that might have scope info
    if "security" in op:
        for sec_req in op["security"]:
            for sec_name, scopes in sec_req.items():
                for scope in scopes:
                    docs += f" {scope} "
    
    # Check if all required scopes are in documentation
    for scope in required_scopes:
        if scope not in docs:
            return False
    
    return True

def is_in_endpoint_list(path: str, method: str, endpoint_list: List[Dict[str, str]]) -> bool:
    """Check if the path/method combo is in the endpoint list."""
    if not endpoint_list:
        return True
    
    for endpoint in endpoint_list:
        if endpoint.get("path") == path and endpoint.get("method").lower() == method.lower():
            return True
    
    return False

# ---------------------------------------------------------------------------
# Action inference
# ---------------------------------------------------------------------------

def infer_action_name(method: str, path: str) -> str:
    if method in HTTP_TO_ACTION:
        return HTTP_TO_ACTION[method]
    if method == "get":
        if path.endswith("/$count") or path.endswith("count"):
            return "getCount"
        return "getOne" if "{" in path else "getMany"
    if method == "put":
        return "update"
    if method == "head":
        return "getOne"
    raise ValueError(f"Unsupported HTTP method: {method}")

# ---------------------------------------------------------------------------
# Schema extraction
# ---------------------------------------------------------------------------

def build_property_block(name: str, schema: Dict[str, Any], definitions: Dict[str, Any] = None) -> Dict[str, Any]:
    prop = {"type": schema.get("type", "string"), "path": f"$.{name}"}
    
    # Add additional metadata if present
    if "format" in schema:
        prop["format"] = schema["format"]
    if "enum" in schema:
        prop["enum"] = schema["enum"]
    if "description" in schema:
        prop["description"] = schema["description"]
    
    # Handle array type
    if schema.get("type") == "array" and "items" in schema:
        items_schema = schema["items"]
        prop["items"] = {"type": "object"}
        
        if "$ref" in items_schema:
            ref_name = items_schema["$ref"].split("/")[-1]
            if definitions:
                referenced_schema = definitions.get(ref_name, {})
                # Include properties from referenced schema
                if "properties" in referenced_schema:
                    prop["items"]["properties"] = {}
                    for prop_name, prop_schema in referenced_schema["properties"].items():
                        prop["items"]["properties"][prop_name] = {
                            "type": prop_schema.get("type", "string"),
                            "path": f"$.{prop_name}"
                        }
                        if "enum" in prop_schema:
                            prop["items"]["properties"][prop_name]["enum"] = prop_schema["enum"]
        else:
            # Handle direct item properties
            if "properties" in items_schema:
                prop["items"]["properties"] = {}
                for prop_name, prop_schema in items_schema["properties"].items():
                    prop["items"]["properties"][prop_name] = {
                        "type": prop_schema.get("type", "string"),
                        "path": f"$.{prop_name}"
                    }
            else:
                # Simple array items
                prop["items"].update({
                    k: v for k, v in items_schema.items()
                    if k not in ("$ref",)
                })
            
    return prop

def extract_body_schema(op: Dict[str, Any], swagger: Dict[str, Any]) -> Tuple[Optional[Dict[str, Any]], Optional[List[str]]]:
    for param in op.get("parameters", []):
        if param.get("in") == "body" and "schema" in param:
            schema = param["schema"]
            definitions = swagger.get("definitions", {})
            
            # Handle direct array type in body
            if schema.get("type") == "array":
                props = {
                    "body": {
                        "type": "array",
                        "path": "$.body",
                        "items": {"type": "object"}
                    }
                }
                
                items_schema = schema.get("items", {})
                if "$ref" in items_schema:
                    ref_name = items_schema["$ref"].split("/")[-1]
                    referenced_schema = definitions.get(ref_name, {})
                    # Include properties from referenced schema
                    if "properties" in referenced_schema:
                        props["body"]["items"]["properties"] = {}
                        for prop_name, prop_schema in referenced_schema["properties"].items():
                            props["body"]["items"]["properties"][prop_name] = {
                                "type": prop_schema.get("type", "string"),
                                "path": f"$.{prop_name}"
                            }
                            if "enum" in prop_schema:
                                props["body"]["items"]["properties"][prop_name]["enum"] = prop_schema["enum"]
                
                return props, [param["name"]] if param.get("required") else []
            
            # Rest of the function for non-array schemas
            if "$ref" in schema:
                ref_name = schema["$ref"].split("/")[-1]
                schema = definitions.get(ref_name, {})
            
            props = {}
            required_fields = schema.get("required", [])
            
            for name, prop_schema in schema.get("properties", {}).items():
                if "$ref" in prop_schema:
                    ref_name = prop_schema["$ref"].split("/")[-1]
                    prop_schema = definitions.get(ref_name, {})
                
                props[name] = build_property_block(name, prop_schema, definitions)
            
            return props or None, required_fields
    return None, None

def extract_param_schema(op: Dict[str, Any], location: str) -> Optional[Dict[str, Any]]:
    props: Dict[str, Any] = {}
    required_fields: List[str] = []
    for param in op.get("parameters", []):
        if param.get("in") == location:
            name = param.get("name")
            props[name] = {
                "type": param.get("type", "string"),
                "path": None if location != "header" else f"$.{name.replace('-', '')}"
            }
            # Add to required fields if parameter is required
            if param.get("required"):
                required_fields.append(name)
    if props:
        return {
            "type": "object", 
            "properties": props,
            "required": required_fields,  # Include required fields list
            "path": None
        }
    return None

# ---------------------------------------------------------------------------
# Documentation and response extraction
# ---------------------------------------------------------------------------

def extract_documentation(op: Dict[str, Any]) -> str:
    lines: List[str] = []
    if title := op.get("summary"): lines.append(f"**{title}**")
    if desc := op.get("description"): lines.append(desc)
    params = op.get("parameters", [])
    if params:
        lines.append("\n**Parameters**")
        for param in params:
            name = param.get("name")
            loc = param.get("in")
            typ = param.get("type") or param.get("schema", {}).get("type", "")
            required = "*required*" if param.get("required") else "optional"
            desc = param.get("description", "")
            lines.append(f"- `{name}` ({typ}, {loc}, {required}): {desc}")
    return "\n".join(lines)

def extract_samples(op: Dict[str, Any]) -> Dict[str, Any]:
    samples = {
        "headers": {},
        "queryParams": None,
        "pathParams": {},
        "body": None
    }
    
    # Extract header samples
    for param in op.get("parameters", []):
        if param.get("in") == "header":
            name = param.get("name", "").lower()
            if "example" in param:
                samples["headers"][name] = [{"$binary": {"base64": param["example"], "subType": "00"}}]
            elif name == "content-type":
                # Default content-type if not specified
                samples["headers"][name] = [{"$binary": {"base64": "YXBwbGljYXRpb24vanNvbg==", "subType": "00"}}]

        # Extract path parameter samples
        elif param.get("in") == "path":
            name = param.get("name")
            if "example" in param:
                samples["pathParams"][name] = param["example"]
            else:
                samples["pathParams"][name] = "sample_value"

        # Extract query parameter samples
        elif param.get("in") == "query":
            if samples["queryParams"] is None:
                samples["queryParams"] = {}
            name = param.get("name")
            if "example" in param:
                samples["queryParams"][name] = param["example"]

        # Extract body samples
        elif param.get("in") == "body" and "schema" in param:
            if "example" in param:
                samples["body"] = param["example"]
            elif "examples" in param:
                samples["body"] = next(iter(param["examples"].values()))

    return samples

def extract_responses(op: Dict[str, Any], swagger: Dict[str, Any]) -> List[Dict[str, Any]]:
    responses = []
    for status_code, response in op.get("responses", {}).items():
        resp_entry = {"statusCode": int(status_code) if status_code.isdigit() else status_code}
        
        # Extract response schema if present
        if "schema" in response:
            schema = response["schema"]
            if "$ref" in schema:
                ref_name = schema["$ref"].split("/")[-1]
                schema = swagger.get("definitions", {}).get(ref_name, {})
            resp_entry["schema"] = schema

        # Extract response examples if present
        if "examples" in response:
            resp_entry["body"] = next(iter(response["examples"].values()))
        
        # Extract response headers if present
        if "headers" in response:
            resp_entry["headers"] = {
                name: header_def.get("type", "string")
                for name, header_def in response["headers"].items()
            }

        # Add description if present
        if "description" in response:
            resp_entry["description"] = response["description"]

        responses.append(resp_entry)
    return responses

# ---------------------------------------------------------------------------
# Payload builder
# ---------------------------------------------------------------------------

def generate_payload(
    *, swagger: Dict[str, Any], path: str, method: str,
    connection_platform: str, definition_id: str,
    platform_version: str, base_url: str
) -> Dict[str, Any]:
    op = swagger["paths"][path][method]
    resource = first_non_template_segment(path)
    action_name = infer_action_name(method, path)
    if action_name not in ALLOWED_ACTIONS:
        raise ValueError(f"Derived actionName '{action_name}' not allowed")

    headers_schema = extract_param_schema(op, "header")
    query_schema = extract_param_schema(op, "query")
    path_schema = extract_param_schema(op, "path")
    body_props, body_required = extract_body_schema(op, swagger)

    payload: Dict[str, Any] = {
        "connectionPlatform": connection_platform,
        "connectionDefinitionId": definition_id,
        "platformVersion": platform_version,
        "key": f"api::{connection_platform}::{platform_version}::{resource}::{action_name}::{path}",
        "title": op.get("summary") or op.get("operationId", "").capitalize(),
        "name": op.get("operationId", f"{action_name}{camel_case(resource)}"),
        "modelName": f"{connection_platform.capitalize()}{camel_case(resource)}",
        "action": method.upper(),
        "actionName": action_name,
        "baseUrl": base_url,
        "path": path,
        "authMethod": {"type": "OAuth"},
        "schemas": {
            "headers": headers_schema,
            "queryParams": query_schema,
            "pathParams": path_schema,
            "body": {"type": "object", "properties": body_props or {}, "required": body_required or [], "path": None} if body_props else None
        },
        "samples": extract_samples(op),
        "responses": extract_responses(op, swagger),
        "paths": {
            "request": {"object": "$.body"},
            "response": {"object": "$", "id": "$.id", "cursor": None}
        }
    }

    payload["knowledge"] = extract_documentation(op)

    payload.update({
        "testConnectionStatus": {"lastTestedAt": 0, "state": "untested"},
        "isDefaultCrudMapping": None,
        "mapping": None,
        "createdAt": {"$numberLong": now_ms()},
        "updatedAt": {"$numberLong": now_ms()},
        "updated": False,
        "version": "1.0.0",
        "lastModifiedBy": "system",
        "deleted": False,
        "changeLog": {},
        "tags": [],
        "active": True,
        "deprecated": False,
        "supported": True,
    })

    return payload

# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(description="Swagger → Inhotel model-defs with docs, samples, and full responses")
    parser.add_argument("--spec", required=True, type=Path,
                        help="Path to a single JSON spec file or a directory containing multiple .json specs")
    parser.add_argument("--platform", required=True)
    parser.add_argument("--definition-id", required=True)
    parser.add_argument("--base-url", required=True)
    parser.add_argument("--bearer", default=os.getenv("INHOTEL_BEARER_TOKEN"),
                        help="JWT token for auth (env: INHOTEL_BEARER_TOKEN)")
    parser.add_argument("--post", action="store_true",
                        help="Whether to POST payloads to the target endpoint")
    parser.add_argument("--target", default="https://platform-backend.inhotel.io/v1/connection-model-definitions",
                        help="Target URL to POST definitions to")
    parser.add_argument("--output", type=Path,
                        help="Directory to write JSON payload files to")
    parser.add_argument("--scopes", nargs="+", default=[],
                        help="Filter endpoints by required scopes (e.g. 'setup.read properties.read')")
    parser.add_argument("--endpoint-list", type=str,
                        help="JSON string of endpoints to include [{'path': '/path', 'method': 'get'}, ...] or path to JSON file containing the endpoint list")
    args = parser.parse_args()

    # Process the endpoint list if provided
    endpoint_list = []
    if args.endpoint_list:
        try:
            # Check if it's a file path (ends with .json or exists as a file)
            if args.endpoint_list.endswith(".json") or Path(args.endpoint_list).is_file():
                try:
                    with open(args.endpoint_list, "r") as f:
                        endpoint_list = json.load(f)
                    print(f"Loaded endpoint list from file: {args.endpoint_list}")
                except FileNotFoundError:
                    print(f"Error: Endpoint list file not found: {args.endpoint_list}")
                    return
                except PermissionError:
                    print(f"Error: Permission denied reading file: {args.endpoint_list}")
                    return
            else:
                # Treat as JSON string
                endpoint_list = json.loads(args.endpoint_list)
        except json.JSONDecodeError as e:
            print(f"Error parsing endpoint list: {e}")
            return

    spec_paths = list(args.spec.glob("*.json")) if args.spec.is_dir() else [args.spec]
    print(f"Processing {len(spec_paths)} spec file(s): {[str(p) for p in spec_paths]}")

    for spec_path in spec_paths:
        print(f"Processing spec file: {spec_path}")
        try:
            swagger = json.loads(spec_path.read_text())
        except Exception as e:
            print(f"Error reading spec file {spec_path}: {e}")
            continue
            
        version = swagger.get("info", {}).get("version", "v1")
        print(f"API version: {version}")
        
        paths_count = len(swagger.get("paths", {}))
        print(f"Found {paths_count} paths in spec")

        processed_count = 0
        for api_path, methods in swagger.get("paths", {}).items():
            for method in methods:
                # Skip non-HTTP methods
                if method in ["parameters", "$ref"]:
                    continue
                
                op = methods[method]
                
                # Filter by required scopes
                if not has_required_scopes(op, args.scopes):
                    print(f"Skipping {method.upper()} {api_path}: doesn't match required scopes")
                    continue
                
                # Filter by endpoint list
                if not is_in_endpoint_list(api_path, method, endpoint_list):
                    print(f"Skipping {method.upper()} {api_path}: not in endpoint list")
                    continue
                
                print(f"Processing {method.upper()} {api_path}")
                processed_count += 1
                
                try:
                    payload = generate_payload(
                        swagger=swagger,
                        path=api_path,
                        method=method,
                        connection_platform=args.platform,
                        definition_id=args.definition_id,
                        platform_version=version,
                        base_url=args.base_url,
                    )
                    payload_json = json.dumps(payload, indent=2)

                    if args.output:
                        prefix = spec_path.stem
                        out_file = args.output / f"{prefix}_{args.platform}_{method}_{api_path.strip('/').replace('/', '_')}.json"
                        out_file.parent.mkdir(parents=True, exist_ok=True)
                        out_file.write_text(payload_json)

                    if args.post:
                        cmd = [
                            "curl", "--location",
                            "--header", "Content-Type: application/json",
                            "--header", f"Authorization: Bearer {args.bearer}",
                            "--data", payload_json,
                            args.target,
                        ]
                        result = subprocess.run(cmd, capture_output=True, text=True)
                        print(f"POST response for {method.upper()} {api_path}: {result.stdout}")
                        if result.stderr:
                            print(f"POST error: {result.stderr}")
                    else:
                        print(f"=== {method.upper()} {api_path} [{spec_path.name}] ===\n{payload_json}\n")
                except Exception as e:
                    print(f"Error processing {method.upper()} {api_path}: {e}")

        print(f"Completed processing {spec_path}: {processed_count} endpoints processed")

if __name__ == "__main__":
    main()
