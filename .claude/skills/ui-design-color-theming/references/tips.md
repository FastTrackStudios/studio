# ui-design-color-theming: full tip list

50 tips distilled from the STA-518 catalogue (YouTube: @whosajid, kolejain). Grouped by source video. Rationale is the catalogue summary, not a transcript.


## Why the 60-30-10 Rule is RUINING Your UI Designs (kolejain, 66oOi9OLMCw)
- **Build four background layers for product interfaces, not one** [general] Apps need several distinct surface levels for nesting sidebars, panels, and cards, so a single background is too thin. Use only a few neutrals plus strokes and text tones.
- **Make the app frame slightly darker than the content area** [general] A large chrome element needs only a small tonal shift to anchor the layout without competing with content. Light mode especially benefits from this subtle difference.
- **Define light cards with a faint edge, not a black border** [general] Heavy dark outlines look harsh, and cards on a near-white background look washed out with only a soft shadow. A faint light-toned edge separates the card cleanly.
- **Build each accent as a full tonal ramp, not one swatch** [general] A ramp gives hover, pressed, and link states consistent hues instead of ad hoc color picks. The main brand shade sits in the middle of the ramp.
- **In dark mode, space surface tones about twice as far apart** [general] Dark values look more alike to the eye than light values, so the same step size makes surfaces blur together. Widening the steps keeps background layers distinct.
- **Dim text and brighten borders in dark mode** [general] Bright text on dark backgrounds causes glare, and borders need more contrast to stay visible. Adjusting both keeps the interface comfortable over long sessions.
- **In dark mode, raised surfaces get lighter, not darker** [general] Elevation reads as lighter surfaces on dark themes, so a card that sits above the page needs a lighter tone or a border. Light-on-dark is the cue users expect for depth.
- **Keep status and destructive colors fixed across brand themes** [general] Success, failure, and in-progress states must stay recognizable even when the brand is monochrome or another hue. Destructive actions should keep a red signal regardless of brand color.
- **Use a full hue spectrum for chart series** [general] Series drawn from one brand tint blur together, and neutral charts are hard to read. A wider hue spread separates categories clearly.
- **Match perceived lightness across hues with a perceptual color space** [general] Equal numeric lightness in common color models still looks uneven across hues, such as bright green versus blue. OKLCH keeps perceived brightness consistent so chart colors balance.
- **Step chart series hue apart at matched lightness and chroma** [general] Fixing lightness and chroma while moving hue gives distinct series with equal visual weight. The transcript suggests roughly a 25 to 30 degree hue step.
- **Theme a palette by shifting its neutrals in OKLCH** [general] Adjusting the same neutral tokens in a perceptual color space produces a themed variant without hand-picking every value. Exact numeric steps are garbled in the captions and are not reproduced here.

## The stupid simple way to learn UI/UX design in exactly 10 minutes (kolejain, 9WVt1CelBfg)
- **Restyle the same layout to change its feel** [general] Adjusting corner radius, color, imagery and texture can make an identical layout feel friendly or premium. Working this way shows how much the style alone conveys.

## Every UI/UX Concept Explained in Under 10 Minutes (kolejain, EcbgbKtOELY)
- **Start from one primary color and build a ramp** [general] Lighter tints work for backgrounds and darker shades for text, and the same ramp can feed chips, states, and charts.
- **Use semantic colors only for meaning** [general] Blue for trust, red for danger, yellow for warning, and green for success give users signals they already understand, so colors should carry purpose.
- **Lower border contrast in dark mode** [general] A bright border on a dark card creates too much contrast, so softening it keeps the card calm.
- **Create depth in dark mode with lighter surfaces instead of shadows** [general] Shadows barely show on dark backgrounds, so raising a card's brightness above the background is what makes it read as elevated.
- **Dim and desaturate bright chips in dark mode** [general] Vivid chips glare on dark surfaces, so reducing saturation and brightness and adjusting the text color keeps hierarchy without harshness.
- **Use rich hues such as deep purples, reds, and greens for dark palettes** [general] Dark mode is not limited to navy and gray, and deeper hues can give a distinct brand personality.
- **Use a gradient overlay to keep text readable over images** [general] A gradient that fades from the image into a solid background protects the text without hiding the picture.

## The 3 dashboard UI flaws that give away you've NEVER built one (kolejain, Ksx9C2-3yMo)
- **Use color only where the data justifies it** [general] Dashboard color should come from the data, not decoration. A colored chip for an urgent event pulls attention to it for a reason.

## 5 SaaS UI/UX mistakes that SCREAM you Vibe Code (kolejain, PDcQJOPby1k)
- **Choose colors deliberately instead of accepting AI defaults** [SaaS web app] AI-generated palettes tend to use bright colors that clash, so picking a restrained set yourself produces a cohesive look.
- **Tone backgrounds around a single hue** [SaaS web app] Shifting the dark backgrounds from blue to green changed the whole mood while keeping the structure intact.

## The one thing vibe coding CAN’T fix about your website (kolejain, RCneB_MQ7qs)
- **Start the visual identity from a single moody image** [landing page] Cropping, darkening and layering a moody photo creates an atmosphere that sets the brand's feeling. It moves the site away from flat, typical light or dark modes.
- **Add subtle noise to create texture and depth** [landing page] A light grain layered over imagery or backgrounds adds a tactile, premium feel. It also helps text contrast against the background.
- **Build a palette from the image colors plus a few accents** [landing page] Earthy tones pulled from the photography keep the palette cohesive. Accent colors from another source add enough contrast to work with.
- **Use the brand colors or an image as the background, with noise for contrast** [landing page] A background drawn from the brand gives the page its base tone. Noise on top helps keep the headline legible.
- **Darken the edges to draw attention to the center** [landing page] A vignette pulls the eye toward the middle of the section where the main message sits. It is a simple way to create a focal point.
- **Use a cream background and graph-paper texture for a clean software feel** [landing page] Dropping the noise and using a light, subtly textured background suits a clean product. It contrasts with the darker hero and keeps the features section airy.

## Everything you need to Design macOS Apps EXACTLY like Apple (beginner friendly) (kolejain, Vy0KKvZJRH8)
- **Respect the system light and dark appearance** [desktop app (macOS utility)] Macs let users choose a mode, and an app that follows that setting feels consistent with the rest of the system.
- **Do not simply invert colors for dark mode** [general] Inverted palettes create harsh contrast, so dark mode should use colors that are closer together and feel refined rather than mirrored.

## The 4 Levels of Landing Page UI/UX Design (kolejain, eMMiLeo_UGI)
- **Limit accent color to purposeful uses** [landing page] Applying a bright accent to everything dilutes its meaning and overwhelms the text. Reserving it for key elements keeps it useful as a signal.
- **Let the product UI supply the accent color and use white buttons** [landing page] Colorful elements inside the product screenshot add vibrancy without decorating the page. Neutral white buttons then stand out against the dark background.
- **Bring the accent color back to the primary CTA with an icon** [landing page] Reintroducing the accent on the main call to action makes it the clearest next step. An icon adds a little extra clarity.
- **Thread color through the page using curated visuals** [landing page] When visuals are chosen deliberately, color can run through the whole page with purpose. Random images make color feel scattered.

## The secret behind weirdly perfect UI designs (kolejain, neE6wOuBIP8)
- **Mute default values so the changed ones stand out** [general] Gray default values recede, which lets the non-default values catch the eye. Scanning a settings list becomes faster because the important differences are visible first.

## The 80% of UI Design - Typography (whosajid, 9-oefwZ6Z74)
- **Dim secondary text by lowering its lightness instead of using an arbitrary gray** [general] Pure white on black already has maximum contrast, so emphasis works by de-emphasizing everything else. Tuning lightness until the text stays readable while the title leads keeps the palette coherent.
- **Use a lighter shade to highlight the active item** [general] Lighter shades on a dark background read as more prominent, so the active tab stands out from its neighbors. Using the same lightness logic keeps the highlight consistent with the rest of the UI.
- **Use a card with a dark tinted background to separate grouped content** [general] A low-lightness tint on the card background sets it apart from the page while keeping the same hue family. It gives the content a clear container.
- **Express colors in HSL so light mode is a simple lightness flip** [general] Using hue, saturation, and lightness means switching to light mode only requires inverting the lightness value rather than picking new hex codes. The approach keeps the palette consistent across both themes.
- **Swap colors through CSS variables when building light mode** [general] Named variables let each color be changed in one place for the theme. This makes switching between dark and light modes systematic rather than manual.

## The Golden Rule of UI Design (whosajid, 9zt__YPULm8)
- **Use color to rank competing buttons** [e-commerce] When several buttons compete for attention, a color difference shows which action is primary. The hierarchy makes the main choice obvious.

## 23 Hacks about Building Websites (whosajid, AmY3db_Qs94)
- **Pick colors with HSL instead of hex** [general] HSL separates hue, saturation, and lightness, so a lighter hover state or a new shade is a single value change rather than a trip to a color picker.
- **Generate shades by adjusting saturation and lightness** [general] Varying saturation and lightness from one base color produces a full range of shades, which is especially useful for dark mode.
- **Build a palette from one hue plus offsets** [general] Primary and secondary colors come from adjusting saturation and lightness, while tertiary and accent colors sit 60 degrees either side of the primary. This gives a balanced combination that can be tested quickly by changing the hue, and the same formula yields dark mode colors.
- **Respect the user's system color scheme** [general] Reading the operating system's light or dark preference lets the site match what the user has already chosen without a manual toggle.
- **Raise the accent lightness on dark backgrounds** [general] When foreground and background are inverted for dark mode, a brighter accent keeps it readable against a dark surface.

## The Easy Way to Design Premium Websites (whosajid, DBJoepxxQdg)
- **Replace background gradients with neutrals plus one chosen accent** [general] A decorative gradient is a common default that makes a site look templated. A grayscale base with a single accent reserved for key actions looks deliberate.
- **Keep the palette to one or two colors** [general] A rainbow of colors signals a lack of decisions, while one accent that suits the product stands out. Two colors is the upper limit the transcript suggests.

## 2 Ways of Building Websites (whosajid, Kkjpvfwhv-c)
- **Save colors and font styles as variables** [general] Central variables keep colors and type styles consistent and make later changes apply everywhere at once.
