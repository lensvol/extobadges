use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::thread::sleep;
use std::time::Duration;
use serde::Deserialize;

use anyhow::{Result};
use badges::{BadgeBuilder, BadgeColor, BadgeStyle};
use select::document::Document;
use select::predicate::{Name, Class, And, Predicate};


const HELP: &str = "\
Scrapes user count information from various browser extension stores.

USAGE:
    extobadges [OPTIONS]

FLAGS:
    -h, --help      Prints help information.

OPTIONS:
    --delay NUMBER  Set delay between each outbound query (default: 1000).
    --dest PATH     Specify path where to put resulting badge SVGs.
    --badges PATH   Specify path to badge information TOML.
";

#[derive(Deserialize, Debug)]
struct ExtensionPages {
    chrome: Option<String>,
    mozilla: Option<String>,
}

#[derive(Debug)]
struct AppArgs {
    delay: u64,
    dest_path: String,
    badges_toml_path: String
}


fn parse_args() -> Result<AppArgs, pico_args::Error> {
    let mut pargs = pico_args::Arguments::from_env();

    if pargs.contains(["-h", "--help"]) {
        print!("{}", HELP);
        std::process::exit(0);
    }

    let args = AppArgs {
        delay: pargs.opt_value_from_str("--delay")?.unwrap_or(1000),
        dest_path: pargs.opt_value_from_str("--dest")?.unwrap_or(".".to_string()),
        badges_toml_path: pargs.opt_value_from_str("--badges")?.unwrap_or("./badges.toml".to_string()),
    };

    return Ok(args)
}


fn extract_mozilla_addon_users(page_contents: &str) -> Option<u32> {
    let document = Document::from(page_contents);

    let card_titles = document.find(
        And(Name("dl"), Class("MetadataCard-list")).descendant(
            And(Name("dt"), Class("MetadataCard-title")))
    );

    for node in card_titles {
        if node.text().trim() == "Users" {
            let user_count_node = node
                .parent()
                .unwrap()
                .children()
                .find(|c| c.name().unwrap_or("") == "dd");

            if let Some(count) = user_count_node {
                let parsed = count.text().parse::<u32>();
                if parsed.is_err() {
                    return None;
                }

                return Some(parsed.unwrap());
            }
        }
    }

    None
}

fn extract_chrome_webstore_users(page_contents: &str) -> Option<u32> {
    let noscript_start = page_contents.find("<noscript>");
    let noscript_end = page_contents.rfind("</noscript>");

    if noscript_start.is_none() || noscript_end.is_none() {
        std::process::exit(-1);
    }

    let start_idx = noscript_start.unwrap();
    let end_idx = noscript_end.unwrap();
    let actual_html = &page_contents[start_idx + 10..end_idx];

    let document = Document::from(actual_html);

    for node in document.find(Name("span")) {
        let title_node = node.attr("title");
        if let Some(title) = title_node {
            if title == node.text() {
                let parts = title.split_whitespace().collect::<Vec<&str>>();
                if parts.len() != 2 {
                    continue;
                }

                let users = parts.first().unwrap().to_string().parse::<u32>();
                if users.is_err() {
                    return None;
                }

                return Some(users.unwrap());
            }
        }
    }

    None
}

fn fetch_page(url: String) -> Result<String> {
    let contents = ureq::get(&url)
        .call()?
        .into_string()?;
    Ok(contents)
}

fn generate_users_badge(pages: &ExtensionPages, delay: u64) -> Result<String, anyhow::Error>{
    let mut total_count = 0;

    if let Some(chrome_id) = pages.chrome.clone() {
        sleep(Duration::from_millis(delay));

        let url = format!("https://chrome.google.com/webstore/detail/{chrome_id}");
        let store_page = fetch_page(url)?;
        let user_count = extract_chrome_webstore_users(&store_page).unwrap_or(0);
        total_count += user_count;
    }

    if let Some(mozilla_id) = pages.mozilla.clone() {
        sleep(Duration::from_millis(delay));

        let url = format!("https://addons.mozilla.org/en-US/firefox/addon/{mozilla_id}/");
        let store_page = fetch_page(url)?;
        let user_count = extract_mozilla_addon_users(&store_page).unwrap_or(0);
        total_count += user_count;
    }

    let badge_svg = BadgeBuilder::new()
        .style(BadgeStyle::Flat)
        .label("users")
        .message(&format!("{}", total_count))
        .message_color(BadgeColor::CustomRgb(0x0, 0x7e, 0xc6))
        .render()
        .expect("failed to render badge");

    Ok(badge_svg)
}

fn main() {
    let config = parse_args().unwrap();

    let mut badges_toml = String::new();
    File::open(config.badges_toml_path)
        .expect("Cannot open badges file!")
        .read_to_string(&mut badges_toml)
        .expect("Failed to read badges info!");

    let badges_info: HashMap<String, ExtensionPages> = toml::from_str(&badges_toml)
        .expect("Failed to parse badges TOML");

    for badge_name in badges_info.keys() {
        println!("Generating badge for '{badge_name}'...");
        let pages = badges_info.get(badge_name).unwrap();
        let badge_svg = generate_users_badge(&pages, config.delay);

        if badge_svg.is_err() {
            println!("Failed to generate badge for '{badge_name}'!");
            continue;
        }

        // TODO: Use proper path buffers
        let mut file = File::create(format!("{}/{}.svg", config.dest_path, badge_name))
            .expect("Failed to create SVG file");
        file.write_all(badge_svg.unwrap().as_ref()).expect("Failed to write SVG contents into file");
    }

}
