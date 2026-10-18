use volt_templater::{Templater, Token};

struct Card {
    name: String,
    creator: String,
}

#[test]
fn readme_example_holds() {
    let templater = Templater::new(vec![
        Token::new("{name}", |card: &Card| card.name.as_str().into()),
        Token::new("{creator}", |card: &Card| card.creator.as_str().into()),
    ]);

    let compiled = templater.compile("{creator} - {name}.png");
    let card = Card {
        name: "Odessa Vale".into(),
        creator: "ahuro1980".into(),
    };
    assert_eq!(compiled.execute(&card), "ahuro1980 - Odessa Vale.png");
}
