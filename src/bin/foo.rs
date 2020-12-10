use std::{thread, time};

fn main() {
    loop {
        println!("foo");
        thread::sleep(time::Duration::from_secs(1));
    }
}