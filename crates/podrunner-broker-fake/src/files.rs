//! The three files of a JIT runner of a scale set that points at the fake
//! broker: `.runner`, `.credentials` (an OAuth client) and
//! `.credentials_rsaparams` (the RSA key that signs its assertion). The
//! files hold no secret of a host: the key is a key of the test.

use serde_json::json;

/// The files, by name, for a runner named `name` and the broker at `base`.
/// `rsaparams` is the JSON of the key (`d`, `dp`, `dq`, `exponent`,
/// `inverseQ`, `modulus`, `p`, `q`, each in base64).
pub fn runner_files(base: &str, name: &str, rsaparams: &str) -> Vec<(String, String)> {
    let runner = json!({
        "agentId": 7, "agentName": name, "poolId": 1, "poolName": "podrunner",
        "serverUrl": base, "serverUrlV2": base, "useV2Flow": true,
        "gitHubUrl": "https://github.com/example-org", "workFolder": "_work",
        "ephemeral": true, "disableUpdate": true,
    });
    let credentials = json!({ "scheme": "OAuth", "data": {
        "clientId": "00000000-0000-0000-0000-000000000007",
        "authorizationUrl": format!("{base}_apis/oauth2/token"),
        "requireFipsCryptography": "False",
    } });
    vec![
        (".runner".into(), runner.to_string()),
        (".credentials".into(), credentials.to_string()),
        (".credentials_rsaparams".into(), rsaparams.into()),
    ]
}
