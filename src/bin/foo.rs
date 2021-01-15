use std::{thread, time};

fn main() {
    loop {
        let foo = std::env::var("foo");
        println!("foo: {}", match foo {
            Ok(v) => v,
            Err(_) => "<error>".to_string(),
        });
        thread::sleep(time::Duration::from_secs(1));
    }
}