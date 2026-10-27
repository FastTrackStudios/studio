# ui-design-motion: full tip list

39 tips distilled from the STA-518 catalogue (YouTube: @whosajid, kolejain). Grouped by source video. Rationale is the catalogue summary, not a transcript.


## The stupid simple way to learn UI/UX design in exactly 10 minutes (kolejain, 9WVt1CelBfg)
- **Favor smaller, tasteful animations over heavy motion** [general] Large, constant movement can overwhelm the content, while restrained motion feels refined. Taste in motion develops toward restraint over time.

## Every UI/UX Concept Explained in Under 10 Minutes (kolejain, EcbgbKtOELY)
- **Use a micro-interaction to confirm a copy action** [general] A small chip sliding up after copying confirms the result more clearly than a button state alone.

## Everything you need to know about Mobile App UI’s in 8 minutes (beginner friendly) (kolejain, Gfsd8NNuD9g)
- **Animate a swipe-back by moving the background** [mobile app] Sliding the background left by about a third and bringing it back on the right produces a smooth back transition. The motion makes the navigation feel continuous.
- **Zoom the background when a bottom sheet opens and closes** [mobile app] Zooming out the background as the sheet rises, and zooming back in on swipe down, keeps the sheet's layering clear. The motion ties the sheet to the screen behind it.

## The one thing vibe coding CAN’T fix about your website (kolejain, RCneB_MQ7qs)
- **Use the eye's focal point to guide motion** [landing page] Animation should move attention toward the element that matters. Thinking about the focal point first keeps motion purposeful.
- **Use a repeated animation motif as part of the brand identity** [landing page] Gradient transitions and pull-away scrolling that recur across the site become a recognizable motion language. Consistency makes the motion feel intentional.
- **Slide and blur the hero away while the next section slides over** [landing page] Moving the old section out with a blur and bringing the new one in over it avoids a hard break. The hero then feels like part of the background.
- **Rotate showcase images on a timer and zoom on hover** [landing page] Automatic rotation gives each important image a chance to be seen without requiring clicks. A slight zoom on hover adds responsiveness.

## Why the best software must LIE to you (kolejain, S8DcTJm8pYM)
- **Show progress toward commitment in pull-to-refresh and swipe-to-delete** [general] The screen shows how close the action is to completing, and the user can reverse it at any point before committing. This makes gesture-based actions feel controllable.

## 4 UI Design Hacks to KILL boring designs (kolejain, SfX43uIubj4)
- **Keep elements static when motion adds nothing** [general] Scroll animations that do not add meaning can be replaced by a static presentation that makes the same point. Motion should be used on purpose.
- **Give illustrations deliberate entrance motion** [general] Instead of a plain fade, elements can rotate in, fly in, or settle with a slow bob, which makes the page feel alive. Easing makes the motion feel more natural.
- **Use parallax within generous margins rather than busy backgrounds** [general] Parallax on floating elements placed in wide margins adds depth without needing layered backgrounds. This keeps the page simpler while still feeling three-dimensional.
- **Animate headline text in sequence to tell the story** [general] Text animations such as a slide-down followed by a replacement word draw attention to copy that would otherwise be static. Staggering lines one after another keeps the sequence readable.

## Everything you need to Design macOS Apps EXACTLY like Apple (beginner friendly) (kolejain, Vy0KKvZJRH8)
- **Animate a collapse into a toast to confirm a save** [desktop app (macOS utility)] Turning the capture panel into a brief toast that slides away gives a clear, delightful end to the action.
- **Give every keyboard action a visible state change** [general] When a shortcut produces no visible change, users assume it failed, so micro-animations or state changes confirm it worked.
- **Expand a collapsed search bar when it is triggered** [desktop app (macOS utility)] Keeping the bar small until needed stays out of the way, and expanding it on click or shortcut gives immediate feedback.

## The 4 Levels of Landing Page UI/UX Design (kolejain, eMMiLeo_UGI)
- **Add purposeful motion beyond a basic hover effect** [landing page] A single hover effect leaves the page static. Motion that responds to the page's content and flow makes it feel alive.
- **Add simple load animations** [landing page] Gentle entrance motion when the page loads adds movement and welcomes the visitor. Keeping it simple avoids distraction.
- **Use subtle hover effects and smooth transitions** [landing page] Small, fluid motion on interactive elements and sliders makes the interface feel responsive. Smoothness matters more than the amount of movement.
- **Use a blur effect for a polished transition** [landing page] A simple blur layered into the transition raises the perceived quality of the page. The change is small but noticeable.
- **Keep the mega menu open and push content aside** [landing page] Instead of a full-screen menu that opens and closes, shifting the content to make room feels smoother and more connected. Side-by-side the difference is clear.

## 23 Hacks about Building Websites (whosajid, AmY3db_Qs94)
- **Use perspective to control rotation intensity** [general] Setting a perspective distance changes how strongly a rotation reads, so a small tilt can look deliberate rather than odd.

## The Easy Way to Design Premium Websites (whosajid, DBJoepxxQdg)
- **Make motion show a real state change** [general] Animation that reflects an actual change in the product, such as pulses on a sync when a value updates, carries meaning. Decorative motion without that link adds little.

## The Easy Way to Do Web Animations (whosajid, hZh-CiPt91w)
- **Use an ease-in-out timing curve for natural motion** [general] Starting slowly, speeding up and then slowing down reads as smooth and organic, while linear motion looks mechanical. Choosing the timing curve is as important as choosing the duration.
- **Use custom cubic-bezier curves when the default easings feel flat** [general] Keyword easings are limited, and a custom curve lets you shape acceleration to fit the feel you want. Curves like these produce smoother transitions than the defaults.
- **Transition the highlight so it does not jump between links** [general] Instantly moving the highlight looks abrupt, so a short slide of about 0.3 seconds makes the change feel intentional. The transition covers both position and width, so it is set to animate all properties together.
- **Give icons a small feedback animation on click** [general] A short grow and glow confirms that the click registered without adding noise. Keeping it brief keeps the interaction feeling responsive.
- **Use a moving light sweep on buttons for hover emphasis** [general] A skewed gradient that slides across the button on hover is a familiar cue for interactivity. It draws attention to the control without changing its layout.
- **Set perspective to make 3D motion look believable** [general] Without a perspective value, a rotation looks like the element is being squashed or stretched rather than moving in depth. A moderate perspective value gives a natural sense of distance.
- **Flip two-sided cards with a settling motion** [general] Separate front and back faces that turn in depth, with the back hidden while facing away, make a card feel physical. A slight overshoot before it settles adds weight and momentum.
- **Reveal sections with a scroll-triggered entrance** [landing page] Tying an animation to scroll position lets content enter as the user reaches it, which makes a long page feel guided. Depth on the entrance helps the section feel three-dimensional.
- **Fly a copy of the product into the cart to confirm an add-to-cart action** [e-commerce] A physical-looking motion from the product to the cart icon makes the result of the click obvious, which is more satisfying than a plain text message. Animating a clone keeps the product grid layout intact.
- **Update the cart count only after the item lands** [e-commerce] Changing the number the moment the user clicks feels out of sync with the motion. Waiting for the flight to finish, then pulsing the cart icon, makes the feedback feel accurate.
- **Move elements along a defined path** [general] Describing the route once and animating a single progress value is simpler than calculating coordinates by hand, and it lets motion follow curves. Setting a clear start and end keeps the motion predictable.
- **Draw outlines in with a stroke animation** [general] Revealing a line along its path looks like ink being drawn, which can guide attention through a shape. It works for icons, logos, and brand moments.
- **Animate a gradient behind an icon mask for a flowing effect** [general] Moving the colors inside a fixed shape gives the impression of a shimmer or flow without changing the outline. The shape acts as a window onto the moving color.
- **Shake invalid inputs and confirm success with the same pattern** [forms] A short shake communicates an error without needing extra text, and reusing the pattern for success keeps the feedback consistent. Triggering both from state classes keeps the logic simple.
- **Reveal text only after a morph animation finishes** [general] Hiding the label on click and showing the new text once the motion ends prevents the text from competing with the animation. The same logic applies in reverse.
- **Keep shape point counts matched when morphing between shapes** [general] Morphing works smoothly only when both shapes have the same number of points, otherwise the outline distorts mid-animation. Using a tool to align the points before animating avoids the distortion.
