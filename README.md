# life

Personal logging app. One text input, LLM parsing into structured entries,
a clean daily timeline. Tracks nutrition and people met.

Live at https://tejasprabhune.github.io/life

## How it works

Type anything ("2 rotis with dal", "met Alex at the coffee shop"). The
backend sends it to Groq with two tool definitions; the model picks
log_nutrition or log_person and fills the fields. Nutrition entries are
grounded against the USDA FoodData Central database and scaled to the
stated portion. Entries without a clean USDA match keep the model's
estimates and a null usda_fdc_id.

## Stack

- backend/: Rust, Axum, sqlx, PostgreSQL. Deployed on Fly.io
  (tejas-life-api.fly.dev) with an attached Fly Postgres cluster.
- frontend/: React, TypeScript, Vite, vanilla CSS. Deployed to GitHub
  Pages from .github/workflows/pages.yml.
- Parsing: Groq (openai/gpt-oss-120b, falls back to
  llama-3.3-70b-versatile), tool calls with required choice.
- Nutrition data: USDA FoodData Central search API.

## Development

Backend (needs Docker for the database):

    docker run -d --name life-pg -e POSTGRES_PASSWORD=life \
      -e POSTGRES_DB=life -p 5433:5432 postgres:16-alpine
    cd backend
    cp .env.example .env   # fill in GROQ_API_KEY
    cargo run --bin life-api

Test the parse loop:

    cargo run --bin life-cli -- "a banana"
    cargo run --bin life-cli -- --list

Frontend (proxies to localhost:8080 via .env.development):

    cd frontend
    npm install
    npm run dev

## Deploy

Backend: `cd backend && fly deploy --remote-only`. Secrets live on the
Fly app: DATABASE_URL (set by `fly postgres attach`), GROQ_API_KEY,
USDA_API_KEY.

Frontend: push to main; the pages workflow builds frontend/ and deploys.

## USDA API key

The app ships with DEMO_KEY, which is rate limited per IP and often
exhausted on Fly's shared egress IPs. Grounding then falls back to model
estimates. For reliable grounding, get a free key at
https://fdc.nal.usda.gov/api-key-signup.html and run:

    fly secrets set USDA_API_KEY=<key> -a tejas-life-api
