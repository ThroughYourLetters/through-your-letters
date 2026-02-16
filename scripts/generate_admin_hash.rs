//! Generate bcrypt hash for admin password.
//!
//! This utility prompts for a password and outputs its bcrypt hash,
//! suitable for use as the ADMIN_PASSWORD_HASH environment variable.
//!
//! Usage:
//!     cargo run --bin generate_admin_hash
//!
//! Then copy the generated hash to your .env file or deployment configuration.

use std::io::{self, Write};

fn main() -> io::Result<()> {
    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║      Admin Password Hash Generator                        ║");
    println!("╚══════════════════════════════════════════════════════════╝\n");

    println!("⚠️  Enter the admin password:");
    print!("> ");
    io::stdout().flush()?;

    let mut password = String::new();
    io::stdin().read_line(&mut password)?;
    let password = password.trim().to_string();

    if password.is_empty() {
        eprintln!("❌ Error: Password cannot be empty");
        return Ok(());
    }

    print!("Confirm password:\n> ");
    io::stdout().flush()?;

    let mut password_confirm = String::new();
    io::stdin().read_line(&mut password_confirm)?;
    let password_confirm = password_confirm.trim().to_string();

    if password != password_confirm {
        eprintln!("❌ Error: Passwords do not match");
        return Ok(());
    }

    println!("\n⏳ Generating hash (this may take a moment)...\n");

    match bcrypt::hash(&password, 12) {
        Ok(hash) => {
            println!("✅ Successfully generated admin password hash!\n");
            println!("┌──────────────────────────────────────────────────────────┐");
            println!("│ Copy this hash to your environment configuration:        │");
            println!("├──────────────────────────────────────────────────────────┤");
            println!("│                                                          │");
            println!("│ ADMIN_PASSWORD_HASH=\"{}\"", hash);
            println!("│                                                          │");
            println!("├──────────────────────────────────────────────────────────┤");
            println!("│ For .env file:                                           │");
            println!("├──────────────────────────────────────────────────────────┤");
            println!("│ ADMIN_PASSWORD_HASH=\"{}\"", hash);
            println!("│                                                          │");
            println!("└──────────────────────────────────────────────────────────┘\n");
        }
        Err(e) => {
            eprintln!("❌ Error: Failed to hash password: {}", e);
        }
    }

    Ok(())
}
