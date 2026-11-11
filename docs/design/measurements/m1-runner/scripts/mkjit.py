#!/usr/bin/env python3
"""Build a SYNTHETIC jitconfig blob (shape from runner Runner.cs:234-256, settings from ConfigurationStore.cs RunnerSettings).

usage: mkjit.py PORT OUTDIR [--v1]
ServerUrl equals ServerUrlV2 on purpose: BrokerMessageListener.cs:126-132 only opens a second (V1 connectionData) connection when they differ.
Writes OUTDIR/jitconfig.b64 (the blob) and prints nothing secret. No real credential is involved: the RSA key is a fresh
throwaway 2048-bit key from openssl, in the JSON layout of RSAParametersSerializable (IRSAKeyManager.cs:44-100):
keys d dp dq exponent inverseQ modulus p q, each a base64 byte array (Newtonsoft default for byte[]).
Sentinel strings make leaks greppable: SENTINEL-CLIENT-ID, SENTINEL-AGENT-NAME.
"""
import base64, json, os, re, subprocess, sys

port, outdir = sys.argv[1], sys.argv[2]
v1 = "--v1" in sys.argv
os.makedirs(outdir, exist_ok=True)
pem = subprocess.run(["openssl", "genrsa", "2048"], capture_output=True, check=True).stdout
txt = subprocess.run(["openssl", "rsa", "-text", "-noout"], input=pem, capture_output=True, check=True).stdout.decode()


def field(name, size):
    m = re.search(r"^%s:\s*\n((?:\s+[0-9a-f:]+\n)+)" % re.escape(name), txt, re.M)
    raw = bytes.fromhex(re.sub(r"[^0-9a-f]", "", m.group(1)))
    raw = raw.lstrip(b"\x00").rjust(size, b"\x00")
    return base64.b64encode(raw).decode()


exp = int(re.search(r"publicExponent: (\d+)", txt).group(1))
rsa = {"d": field("privateExponent", 256), "dp": field("exponent1", 128), "dq": field("exponent2", 128),
       "exponent": base64.b64encode(exp.to_bytes((exp.bit_length() + 7) // 8, "big")).decode(),
       "inverseQ": field("coefficient", 128), "modulus": field("modulus", 256),
       "p": field("prime1", 128), "q": field("prime2", 128)}
base = "http://127.0.0.1:%s" % port
runner = {"AgentId": 4242, "AgentName": "SENTINEL-AGENT-NAME", "PoolId": 1, "PoolName": "Default",
          "DisableUpdate": True, "Ephemeral": True, "ServerUrl": base + "/v2", "GitHubUrl": base + "/gh",
          "WorkFolder": "_work", "UseV2Flow": not v1, "ServerUrlV2": base + "/v2"}
creds = {"Scheme": "OAuth", "Data": {"clientId": "SENTINEL-CLIENT-ID", "authorizationUrl": base + "/oauth/token",
                                     "oauthEndpointUrl": base + "/oauth/token"}}
b64 = lambda b: base64.b64encode(b).decode()
files = {".runner": b64(json.dumps(runner).encode()),
         ".credentials": b64(json.dumps(creds).encode()),
         ".credentials_rsaparams": b64(json.dumps(rsa).encode())}
blob = b64(json.dumps(files).encode())
open(os.path.join(outdir, "jitconfig.b64"), "w").write(blob)
open(os.path.join(outdir, "rsa.d.b64"), "w").write(rsa["d"])  # for leak greps; throwaway key
print("blob length:", len(blob), "files:", list(files))
