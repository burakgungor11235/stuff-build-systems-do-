mod bs;
mod markup;

use std::fs;

use anyhow::Result;

use crate::bs::{ builder::Builder, config::Manifest};

fn main() -> Result<()> {
    // Load the manifest from stuff.toml
    let manifest = match Manifest::load("test_envs/basic/stuff.toml") {
        Ok(o) => {o},
        Err(e) => {
            let s = fs::read_to_string("test_envs/basic/stuff.toml").unwrap(); 
            println!("{}", s);
            return Err(e);
        },
    };
    
    // Create a builder and run the build
    let mut builder = Builder::new(manifest)?;
    builder.build()?;
    
    Ok(())
}
