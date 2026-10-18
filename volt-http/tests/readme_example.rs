use volt_http::middleware::{Anonymous, Bearer, MiddlewareResult, Token, TokenSource};
use volt_http::{Client, Emulation};

struct Login;

#[async_trait::async_trait]
impl TokenSource for Login {
    async fn fetch(&self) -> MiddlewareResult<Token> {
        Ok(Token::jwt("eyJ".to_owned()))
    }
}

#[test]
fn readme_example_builds() {
    let base = Client::builder()
        .emulation(Emulation::Chrome131)
        .cookie_store(true)
        .build()
        .unwrap();
    let api = base.derive().middleware(Bearer::new(Login)).build();

    let private = api.get("https://example.com/api/me");
    let public = api
        .get("https://example.com/api/cards/1")
        .extension(Anonymous);
    drop((private, public));
}
