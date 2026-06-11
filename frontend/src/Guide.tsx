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
        <h2>music</h2>
        <p>
          Log an album after a full listen, or a single song. Songs heard in public with no
          known title go into a revisit queue; describe what you heard and where. Albums get
          rated by comparison, not numbers: hit <em>rate</em> on an album row, pick a tier,
          then answer a few this-or-that matchups. The 0 to 10 score comes from where the
          album lands among everything else you've ranked.
        </p>
        <ul>
          <li>listened to Blonde by Frank Ocean, loved the back half</li>
          <li>obsessed with Idioteque by Radiohead</li>
          <li>catch that dreamy synth song playing at the cafe right now</li>
        </ul>
        <p className="schema">
          album fields: title, artist, thoughts, rating. song fields: title, artist, status,
          context, source, thoughts
        </p>
      </section>

      <section>
        <h2>gym</h2>
        <p>
          Log the session itself in wger during the workout. Then say "worked out" here and
          the latest wger session is pulled in with its exercises, sets and volume. Only
          sessions from today are logged unless you explicitly ask otherwise. Set data is
          owned by wger; the note field is editable here.
        </p>
        <ul>
          <li>worked out</li>
          <li>worked out, felt strong on squats</li>
          <li>log my last gym session even though it was yesterday</li>
        </ul>
        <p className="schema">
          fields: date, exercises and sets (read-only), impression, duration, note
        </p>
      </section>

      <section>
        <h2>places</h2>
        <p>
          Log venues separately from food: the spot, what you ordered, what you thought.
          Places get the same comparison-based rating as albums, ranked within their
          category (coffee, restaurant, bar, dessert).
        </p>
        <ul>
          <li>went to Blue Bottle, got a cortado, solid but pricey</li>
        </ul>
      </section>

      <section>
        <h2>travel</h2>
        <p>
          Log destinations with a small itinerary, then add to it later. Trips rank like
          albums.
        </p>
        <ul>
          <li>went to Lisbon, did tram 28, Belem, lots of pastel de nata</li>
          <li>in Lisbon, add LX Factory</li>
        </ul>
      </section>

      <section>
        <h2>sleep</h2>
        <p>
          Two phrases open and close a night: the entry shows a quiet sleeping state until
          you wake, then morphs into the duration. State a time and it is used instead of
          the clock.
        </p>
        <ul>
          <li>sleeping now</li>
          <li>just woke up</li>
          <li>went to bed at 11, just woke up</li>
        </ul>
      </section>

      <section>
        <h2>learning</h2>
        <p>
          Set up a field in the learning tab: add PDFs, links or named resources, then
          generate and edit a topic plan. Daily progress is logged here in the input;
          names resolve against your configured fields and resources.
        </p>
        <ul>
          <li>watched CS 285 lecture 7</li>
          <li>did 6 implementation problems on actor-critic, felt shaky</li>
          <li>read chapter 3 of Sutton and Barto, feeling confident</li>
        </ul>
      </section>

      <section>
        <h2>voice</h2>
        <p>
          Hold the mic button to speak instead of typing. The transcript is cleaned up
          before it lands: fillers and false starts go, self-corrections apply ("at 7,
          no wait, 8" becomes "at 8"), spoken emails become addresses. It drops into the
          textbox for a quick glance before you submit with enter or the arrow.
        </p>
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
