export function Guide() {
  return (
    <div className="app guide">
      <header>
        <h1 className="brand">guide</h1>
        <a className="guide-link" href="#/">
          back
        </a>
      </header>

      <section>
        <h2>how it works</h2>
        <p>
          Type anything into the input and press enter. The entry is parsed into one of two
          types, food or person, and shows up in the day's log. Click any row to edit its
          fields or delete it.
        </p>
      </section>

      <section>
        <h2>food</h2>
        <p>
          Name the food and the amount. Quantities can be loose ("a bowl of", "2 rotis",
          "150g"). Calories and macros are looked up in the USDA FoodData Central database
          and scaled to your portion. Entries without a clean USDA match are estimated and
          marked <em>estimated</em> in the editor.
        </p>
        <ul>
          <li>a banana</li>
          <li>2 rotis with dal</li>
          <li>butter chicken with naan at Vik's</li>
          <li>half a cup of cold brew with oat milk</li>
        </ul>
        <p className="schema">
          fields: food, quantity, cals, protein, carbs, fat
        </p>
      </section>

      <section>
        <h2>people</h2>
        <p>
          Start with who you met, then add anything worth remembering. Emails and phone
          numbers are picked out automatically; everything else lands in context.
        </p>
        <ul>
          <li>met Alex Chen at the coffee shop, works on robotics, alex@berkeley.edu</li>
          <li>ran into Priya from high school at BART, catching up next week</li>
        </ul>
        <p className="schema">fields: name, email, phone, context</p>
      </section>

      <section>
        <h2>tips</h2>
        <ul>
          <li>One thing per entry parses best. Log a meal as separate items for cleaner macros.</li>
          <li>Use the arrows next to the date to browse past days.</li>
          <li>Numbers are editable. Click a row and correct anything the parser got wrong.</li>
        </ul>
      </section>
    </div>
  )
}
