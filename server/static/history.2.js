document.addEventListener("DOMContentLoaded", function(event) {
  var loadGraphBtn = document.getElementById("loadGraphBtn");
  if (loadGraphBtn) {
    loadGraphBtn.addEventListener("click", function() {
      var overlay = document.getElementById("overlay");
      if (overlay) {
        overlay.style.display = "none";
        loadGraph();
      }
    });
  }
});

async function loadGraph() {
    let graphDiv = document.getElementById('history');
    try {
        let g = new Dygraph(
            graphDiv,
            "/api/graph",
            {
                title: 'Historic Instance Healthiness',
                showRangeSelector: true,
                width: '100%',
                rangeSelectorPlotFillColor: 'MediumSlateBlue',
              rangeSelectorPlotFillGradientColor: 'rgba(123, 104, 238, 0)',
              colorValue: 0.9,
              fillAlpha: 0.4,
              colors: ['#008000', '#ffa500'],
            }
        );
        g.ready(function() {
            g.setAnnotations([
            {
              series: "Healthy",
              x: "2023-08-15T07:10:17Z",
              shortText: "G",
              text: "First API Change"
            },
            {
              series: "Dead",
              x: "2023-10-21T14:37:44Z",
              shortText: "C",
              text: "Wiki Cleanup"
            },
            {
                series: "Healthy",
                x: "2024-01-25T15:24:16Z",
                shortText: "R",
                text: "API Shutdown"
              }
            ]);
          });
    } catch (error) {
        console.error('Failed to fetch data:', error);
        graphDiv.textContent = "Failed to load data.";
    }
}