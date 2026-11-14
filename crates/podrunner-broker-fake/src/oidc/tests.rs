use super::*;

fn decode(part: &str) -> Value {
    serde_json::from_slice(&URL_SAFE_NO_PAD.decode(part).unwrap()).unwrap()
}

#[test]
fn a_token_has_the_claims_and_a_signature_of_the_key_of_the_run() {
    let issuer = Issuer::new("https://oidc.podrunner.invalid/").unwrap();
    let claims: Map<String, Value> = serde_json::from_value(json!({
        "sub": "repo:me/app:ref:refs/heads/main", "repository": "me/app", "repository_owner": "me",
    }))
    .unwrap();
    let (status, body) =
        route("GET", "/_oidc/token?api-version=2.0&audience=sts.example%2Fa+b", &issuer, Some(&claims)).unwrap();
    assert_eq!(status, 200);
    let token = serde_json::from_slice::<Value>(&body).unwrap()["value"].as_str().unwrap().to_string();
    let parts: Vec<&str> = token.split('.').collect();
    assert_eq!(parts.len(), 3);
    let (head, c) = (decode(parts[0]), decode(parts[1]));
    assert_eq!(head["alg"], "ES256");
    assert_eq!(c["iss"], "https://oidc.podrunner.invalid");
    assert_eq!(c["aud"], "sts.example/a b");
    assert_eq!(c["sub"], "repo:me/app:ref:refs/heads/main");
    assert!(c["exp"].as_u64().unwrap() > c["iat"].as_u64().unwrap());
    // The signature holds for the key of the JWK set, and only for it.
    let jwks = issuer.jwks();
    let key = &jwks["keys"][0];
    assert_eq!(key["kid"], head["kid"]);
    let mut point = vec![4u8];
    point.extend(URL_SAFE_NO_PAD.decode(key["x"].as_str().unwrap()).unwrap());
    point.extend(URL_SAFE_NO_PAD.decode(key["y"].as_str().unwrap()).unwrap());
    let public = ring::signature::UnparsedPublicKey::new(&ring::signature::ECDSA_P256_SHA256_FIXED, &point);
    let sig = URL_SAFE_NO_PAD.decode(parts[2]).unwrap();
    let input = format!("{}.{}", parts[0], parts[1]);
    assert!(public.verify(input.as_bytes(), &sig).is_ok());
    assert!(public.verify(b"another input", &sig).is_err());
    // No audience: the URL of the owner, as GitHub.
    let (_, body) = route("GET", "/_oidc/token?api-version=2.0", &issuer, Some(&claims)).unwrap();
    let t = serde_json::from_slice::<Value>(&body).unwrap()["value"].as_str().unwrap().to_string();
    assert_eq!(decode(t.split('.').nth(1).unwrap())["aud"], "https://github.com/me");
    // A job with no permission gets no token; another issuer has another key.
    assert_eq!(route("GET", "/_oidc/token?x=1", &issuer, None).unwrap().0, 403);
    assert_ne!(Issuer::new("https://x.invalid").unwrap().jwks()["keys"][0]["x"], key["x"]);
    let (_, d) = route("GET", "/.well-known/openid-configuration", &issuer, None).unwrap();
    assert!(String::from_utf8(d).unwrap().contains("https://oidc.podrunner.invalid/.well-known/jwks"));
    assert!(route("GET", "/message", &issuer, None).is_none());
}
