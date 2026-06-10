use std::env;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let api = env::var("LIFE_API_URL").unwrap_or_else(|_| "http://localhost:8080".into());
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: life-cli <text to log>");
        eprintln!("       life-cli --list [YYYY-MM-DD]");
        std::process::exit(1);
    }

    let http = reqwest::Client::new();

    if args[0] == "--list" {
        let mut url = format!("{api}/api/logs");
        if let Some(date) = args.get(1) {
            url = format!("{url}?date={date}");
        }
        let logs: serde_json::Value = http.get(&url).send().await?.error_for_status()?.json().await?;
        println!("{}", serde_json::to_string_pretty(&logs)?);
        return Ok(());
    }

    let text = args.join(" ");
    let started = std::time::Instant::now();
    let resp = http
        .post(format!("{api}/api/logs"))
        .json(&serde_json::json!({ "raw_text": text }))
        .send()
        .await?;
    let status = resp.status();
    let body: serde_json::Value = resp.json().await?;
    println!("{status} in {:?}", started.elapsed());
    println!("{}", serde_json::to_string_pretty(&body)?);
    Ok(())
}
