use std::{io::{self, Write}, error::Error};

pub fn get_user_input(s: &str) -> Result<String, Box<dyn Error>> {
    print!("{}", s);
    let mut input = String::new();
    io::stdout().flush()?;
    io::stdin().read_line(&mut input)?;
    let user_selection: String = input.trim().to_string();
    Ok(user_selection)
}

pub fn select_element(s: &str, len: usize) -> usize {
    loop {
        match get_user_input(s) {
            Ok(res) => 
                match res.parse::<usize>() {
                    Ok(i) => 
                        if i >= len {
                            println!("[select_element][error]: out of bounds.") 
                        } else {
                            break i
                        },
                    Err(e) => println!("[select_element][error]: {}", e),
                },
            Err(e) => println!("[select_element][error]: {}", e),
        };
        println!("Try again.")
    }
}

