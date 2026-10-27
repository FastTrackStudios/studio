# ui-design-components-interaction: full tip list

81 tips distilled from the STA-518 catalogue (YouTube: @whosajid, kolejain). Grouped by source video. Rationale is the catalogue summary, not a transcript.


## The stupid simple way to learn UI/UX design in exactly 10 minutes (kolejain, 9WVt1CelBfg)
- **Keep the navbar small and out of the way** [landing page] A compact navigation bar lets the hero take the focus while still giving users access to links. Modern navs follow this pattern.

## Every UI/UX Concept Explained in Under 10 Minutes (kolejain, EcbgbKtOELY)
- **Show selected and inactive states visually** [general] A highlighted container marks the current choice, and greyed-out text signals that an item cannot be used right now.
- **Let the interface signify what each element does** [general] Pressed states, highlighted active nav items, hover states, and tooltips tell users what is possible, so instructions are not needed.
- **Use icons and alignment instead of from and to labels** [general] A route shown with icons and aligned rows communicates direction without extra words.
- **Keep light-mode shadows soft** [general] Reducing shadow opacity and increasing blur makes depth subtle, and if the shadow is the first thing noticed it is too strong.
- **Give popovers stronger shadows than cards** [general] Content that floats above other content needs more shadow to show its layering, while cards need less.
- **Use inner and outer shadows for tactile buttons** [general] Combining both shadow types can give buttons a raised, physical feel.
- **Size icons to the font line height** [general] Matching icon size to the text's line height, such as 24 pixels, keeps icons balanced with the text beside them.
- **Use ghost buttons for secondary actions** [general] A button with no background until hovered reads as quiet, and it pairs well with a primary button; padding should be proportioned around a roughly 2:1 width to height ratio.
- **Give buttons default, hover, pressed, disabled, and loading states** [general] Each state tells users what the button is doing or whether it can be used.
- **Give inputs focus, error, and warning states** [general] A focus state confirms the active field, and red borders with messages make problems clear.
- **Add progressive blur on top of the gradient for a modern finish** [general] A progressive blur layered over the gradient adds polish while keeping text legible.

## Everything you need to know about Mobile App UI’s in 8 minutes (beginner friendly) (kolejain, Gfsd8NNuD9g)
- **Keep tap targets at least 44 pixels** [mobile app] People have varied finger sizes, so a 44-pixel minimum target keeps controls easy to hit. This applies to bottom bar items and other buttons.
- **Let top bar actions change with the current page** [mobile app] A bell and a more menu are a reasonable default, but the useful actions depend on context. Adapting the top bar to each page keeps it relevant.
- **Use cards as the main way to group content** [mobile app] Cards group related content in place of white space, which is scarce on a small screen. Almost everything on a mobile screen ends up inside some kind of card.
- **Use a bottom sheet to keep the user in context for a choice** [mobile app] Selecting a template while editing a note would otherwise pull the user away to a new page. A bottom sheet with a title, search, and confirm and close actions keeps them in place.
- **Make a plus button open a small menu or go straight into input** [mobile app] The plus button is the main action, so it should either reveal a short menu or open an input right away. Either choice reduces the steps needed to start creating.
- **Offer swipe-up to search as a quick entry point** [mobile app] Swiping up to reveal search, as Slack and Apple do, gives a fast route to search without using a visible control. It works best when paired with the education in the previous tip.
- **Use long press for contextual actions with a blurred background** [mobile app] Long press is the mobile equivalent of right click. Blurring the rest of the screen and slightly zooming the element focuses attention on the actions, and a preview can go further.

## The 3 dashboard UI flaws that give away you've NEVER built one (kolejain, Ksx9C2-3yMo)
- **Let the data shape table columns** [general] Categorical values such as department or status read faster as chips, and numbers align by place value when right-aligned. Both changes make a table easier to scan.
- **Design the hidden affordances that appear on interaction** [general] Features such as copy chips over cells or small comment markers are not visible in the static layout but are needed for the table to work. Plan them as part of the UI rather than as an afterthought.
- **Add tooltips to icons and ambiguous labels** [general] Users cannot be assumed to understand every icon or label, and tooltips give them more detail on demand. This matters most in beginner dashboards.

## 5 SaaS UI/UX mistakes that SCREAM you Vibe Code (kolejain, PDcQJOPby1k)
- **Replace emojis with professional icons** [SaaS web app] Emojis in interface cards look casual, and swapping them for clean icons makes the product read as more professional.
- **Use one consistent icon library** [SaaS web app] A single icon style, such as Phosphor or Lucide, keeps the interface visually unified.
- **Replace decorative icons with micro charts** [SaaS web app] Small charts carry real information and add color, which is more useful than icons and buttons that only decorate.
- **Replace generic gradient avatar circles with an account card** [SaaS web app] A card showing the account gives more useful context than a decorative letter-in-a-circle avatar.
- **Collapse card actions into an overflow menu** [SaaS web app] Fewer visible buttons make each card calmer, and the date and metrics can be rearranged to take the space.
- **Use a modal for sparse create forms** [SaaS web app] A form with few fields in a large area looks empty, so a focused modal suits it better and leaves room for future options.
- **Collapse advanced form options by default** [SaaS web app] Hiding optional settings keeps the common path short while still making advanced controls available.
- **Add a toggle to split grouped items into individual links** [SaaS web app] Letting users view grouped items separately makes comparisons easy with little build effort.
- **Add informative icons with color to dense rows** [SaaS web app] Small colored icons next to useful data make each row scannable and add visual interest.

## The one thing vibe coding CAN’T fix about your website (kolejain, RCneB_MQ7qs)
- **Derive icons from the brand accents** [landing page] Icons drawn in the brand's accent colors and style represent product features consistently. They tie the page back to the identity.

## Why the best software must LIE to you (kolejain, S8DcTJm8pYM)
- **Keep a submenu open while the pointer moves diagonally toward it** [general] An invisible triangle from the pointer to the submenu keeps it open during a diagonal move, so it does not close prematurely. The pointer's speed matters too, so a slow move is treated as moving to the item above.
- **Show tooltips instantly for adjacent items after the first wait** [general] After one hover delay, neighboring controls reveal their tooltips immediately, since the user is likely trying to learn what each one does. This saves waiting for each control in turn.
- **Expand a collapsed sidebar only when the pointer stays at its edge** [general] Requiring a sustained hover on the edge avoids accidental expansion while still opening the sidebar for deliberate use. The rule trades certainty for convenience, and fast passes do not trigger it.
- **Preview the drop position and autoscroll with an edge gradient when reordering** [general] Instead of a binary choice between placing an item or scrolling, the interface shows the predicted landing spot and scrolls faster the closer the pointer gets to the edge. This handles uncertain intent gracefully.
- **Use an undo toast instead of a confirmation modal for reversible actions** [general] Most people mean what they just did, so a modal makes nearly everyone confirm something they already intended. Acting immediately and offering undo only interrupts the rare mistake.
- **Autosave instead of showing save and cancel buttons** [general] Saving periodically or on change removes buttons that users rarely need to think about. The interface assumes the user will save, and does it for them.
- **Keep a warning modal for novel or irreversible actions** [general] Undo does not cover actions like sending bulk email or money, so those still need explicit confirmation. Reserve the modal for actions that cannot be reversed.
- **Commit a press-and-release button on release so users can cancel by moving off** [general] Most buttons already work this way, giving users a built-in cancel even if they rarely use it deliberately. Like a camera shutter, the action is only committed when the press is completed.
- **Let buttons that open a popover or menu open on press** [general] Opening on press lets the user press, drag, and release onto the target item in one motion. These controls are toggles rather than commit-on-release buttons, which is why they behave differently.

## Everything you need to Design macOS Apps EXACTLY like Apple (beginner friendly) (kolejain, Vy0KKvZJRH8)
- **Use a native-style blur on floating panels** [desktop app (macOS utility)] A background blur, like the one Paste uses, makes a floating panel feel like part of the OS.
- **Keep search prominent and always reachable** [general] Native Mac apps put search in an obvious, always-available place, so users expect to find it quickly.
- **Put the result count, clear, and back controls in the search bar** [general] Because browse and search views look alike, a count, a clear button, and a back arrow in the bar tell the user what state they are in.
- **Put frequent actions in a floating action bar** [general] Copy, find, and delete within easy reach save the user from hunting for them, and they feel natural rather than like features.
- **Use a simple modal onboarding with micro-interactions** [desktop app (macOS utility)] Even a desktop app benefits from a short guided start, and small animations make the modal feel polished.

## The 4 Levels of Landing Page UI/UX Design (kolejain, eMMiLeo_UGI)
- **Match corner rounding between images and buttons** [landing page] Sharp-cornered images next to fully rounded buttons look disconnected. A consistent corner style makes the page feel like one system.
- **Mark the most important nav actions with an outline** [landing page] An outlined style sets primary actions apart from plain text links. It gives the menu a visible hierarchy without extra clutter.
- **Add hover drop-downs to the most important nav links** [landing page] Small menus that open on hover signal that the main links lead to more content. They add depth without crowding the bar.
- **Make feature sets interactive with multi-select tabs** [landing page] Letting visitors click through and toggle features adds variety and invites exploration. It replaces a static row of cards.
- **Use a mega menu for the most important links** [landing page] A large menu panel signals that the product has depth and invites exploration. It organizes many links without overwhelming the top bar.
- **Reveal a call to action only on hover** [landing page] A button that appears only when needed keeps the logo section clean. It still offers the action when the visitor shows interest.

## The secret behind weirdly perfect UI designs (kolejain, neE6wOuBIP8)
- **Move content or create new edges instead of hiding controls in a menu** [general] Collapsing secondary buttons into a corner menu hides functionality and does not really fix the layout. Moving content or adding a new line that aligns controls gives the area a clear edge again.
- **Use placement to make controls self-explanatory** [general] A checkbox sitting above a row of actions is understood from its position alone, while unlabelled neighbors force people to guess or wait for a tooltip. Position can carry meaning that otherwise would need text.
- **Use recognizable affordances such as link color, underlines, and chips** [general] Blue underlined links and category chips signal what is interactive and what a tag is. Showing chips with icons also lets people edit tags directly rather than hunting for them in a hidden menu.
- **Show checked state with a fill and checkmark rather than color alone** [general] An unchecked box stays neutral with a thin border, and a checked box gains a fill and a checkmark. The change is visible through shape and icon, not only through a color that would otherwise be the only signal.

## Original Ideas are Overrated (whosajid, 41IMbwQR-dc)
- **Refine proven interface patterns instead of inventing new ones** [general] Existing layouts and interactions can be polished, modernized, or made faster and cleaner. This is presented as the practical form of creativity in UI work.

## The 80% of UI Design - Typography (whosajid, 9-oefwZ6Z74)
- **Use rem units for font sizes so users can scale text** [general] Relative units respect the user's browser font settings, which supports accessibility guidelines. Pixel sizes lock text to one size regardless of user preference.

## The Golden Rule of UI Design (whosajid, 9zt__YPULm8)
- **Make links look like links and buttons look like buttons** [general] Users arrive with expectations about what is clickable, and matching them lets people act without learning the interface. Breaking those expectations forces users to stop and think.
- **Follow established conventions for navigation and icons** [general] Familiar patterns, such as a search icon for search or a cart icon for checkout, let users predict behavior. Conventions save users effort even when they make a design look less novel.
- **Add a search bar to skip deep navigation** [e-commerce] Users who know what they want can go straight there instead of clicking through categories. Search shortens the journey for impatient visitors.
- **Add filters and sorting when a catalog is large** [e-commerce] Long lists force users to think through many options, so filters and sorting narrow the choices. The page then becomes easy to scan without repeated back-and-forth.
- **Use a photo carousel to avoid scrolling through images** [e-commerce] Showing several photos in a carousel keeps the product details visible without forcing a long scroll. It gives shoppers the views they expect in one compact area.
- **Underline text that is clickable** [e-commerce] Ratings and other interactive text look like plain text unless marked, so underlining signals that they can be clicked. Small cues like this prevent guessing.

## 23 Hacks about Building Websites (whosajid, AmY3db_Qs94)
- **Use SVG for icons and illustrations** [general] SVGs are lightweight, behave like other HTML elements for styling, and can respond to dark mode.
- **Disable a section without hiding it** [general] The inert attribute keeps content visible while making its buttons and inputs non-interactive, and screen readers skip it. This suits states where a section is temporarily unavailable.
- **Use the dialog element for popups** [general] A native dialog hides its content until it is opened, can be shown as a modal, and closes with Escape or a close button, which gives popups correct behavior without custom code.
- **Style list markers directly in CSS** [general] List markers can be styled in one line of CSS, which avoids pseudo elements and manual alignment of text with the marker.
- **Limit hover effects to devices that support hover** [general] Hover styles can get stuck or fail on touch screens, so wrapping them in a media query lets touch devices get a separate treatment.
- **Use details and summary for collapsibles** [general] The native disclosure element hides and reveals content without a button or extra scripting.
- **Show extra info on hover with the title attribute** [general] The title attribute gives any element a built-in tooltip, which provides supplementary text without custom UI.
- **Add suggestions to an input with datalist** [general] A datalist adds autocomplete options to a text field, so users can type or pick from suggestions without custom CSS or JavaScript.
- **Use native progress and meter elements** [general] The built-in progress and meter elements display values as bars without extra markup or styling, and the video notes they serve slightly different purposes.

## The Easy Way to Design Premium Websites (whosajid, DBJoepxxQdg)
- **Make the hero demonstrate the product itself** [general] A live, interactive example of what the product does communicates its value better than an illustration. The centerpiece should show the thing visitors came to understand.

## 2 Ways of Building Websites (whosajid, Kkjpvfwhv-c)
- **Keep the UI consistent with a shared design system** [general] Repeated patterns across pages look coherent and are easier to build and maintain. The video frames consistency as a win for both users and developers.
- **List the repeating elements before designing** [general] A design system is a collection of repeated elements, so cataloguing buttons, sections, and cards first defines what the system needs. Colors and fonts can be handled as separate decisions.
- **Turn repeated sections and buttons into reusable components** [general] Once a pattern is a component, a page can be assembled by dropping in components and replacing the content. This keeps layouts consistent and quick to build.
- **Apply the same component inventory to web app interfaces** [web app] Sidebars, buttons, and feed cards in an app are repeating elements just like a landing page's sections. Building them as components keeps the app consistent.

## The Easy Way to Do Web Animations (whosajid, hZh-CiPt91w)
- **Use one sliding highlight behind navigation links** [general] A single background that slides and reshapes to fit the hovered link feels fluid and shows clearly which item is active. Measuring each link and moving the highlight to match keeps it accurate for links of different widths.
- **Stack secondary options under the main button and reveal them on click** [general] Hiding the extra options beneath the primary button, shrunk down, makes the control look like one button until the user needs more. Expanding them on click keeps the interface compact.
- **Trigger morph and animated controls from a real button, not the graphic** [general] A native button is keyboard-focusable and announced to assistive technology, while an SVG click target is not. A transparent button placed over the graphic keeps the visual the same and is more accessible.
