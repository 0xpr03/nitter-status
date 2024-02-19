document.addEventListener("DOMContentLoaded", function(event) {
    loadGraph();
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
                /*rollPeriod: 14,
                showRoller: true,
                customBars: true,
                legend: 'always',
                
                rangeSelectorPlotFillColor: 'MediumSlateBlue',
                rangeSelectorPlotFillGradientColor: 'rgba(123, 104, 238, 0)',
                colorValue: 0.9,
                fillAlpha: 0.4*/
            }
        );
        g.ready(function() {
            g.setAnnotations([
            {
              series: "Healthy",
              x: "2023-05-18",
              shortText: "G",
              text: "First API Change"
            },
            {
                series: "Healthy",
                x: "2024-01-24",
                shortText: "D",
                text: "API Shutdown"
              }
            ]);
          });
    } catch (error) {
        console.error('Failed to fetch data:', error);
        graphDiv.textContent = "Failed to load data.";
    }
}