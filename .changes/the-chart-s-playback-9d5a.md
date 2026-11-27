---
"engine-host" = "patch"
"engraver" = "patch"
---

The chart's playback cursor follows the bars as played: on a repeat's second pass it goes back to the repeated bars, and past the chart's written end it is still on the bar being played. ChartView's page, band and smooth cursor (what the chart panel follows) and the browser chart pane look the time up in the chart's played bars (bar_times) and take only x from the layout (studio#450).
