// SPDX-License-Identifier: AGPL-3.0-only
document.addEventListener("DOMContentLoaded", function (event) {
  var loadGraphBtn = document.getElementById("loadGraphBtn");
  if (loadGraphBtn) {
    loadGraphBtn.addEventListener("click", function () {
      var elements = document.querySelectorAll('.graph');
      elements.forEach(function (element) {
        element.style.display = 'block';
      });
      loadGraphBtn.style.display = "none";
      loadHealthGraph();
      loadStatsGraph();
    });
  }
});

async function loadHealthGraph() {
  let graphDiv = document.getElementById('graph-health');
  try {
    let g = new Dygraph(
      graphDiv,
      "/api/csv/health",
      {
        title: 'Historic Instance Healthiness',
        showRangeSelector: true,
        width: '100%',
        rangeSelectorPlotFillColor: 'MediumSlateBlue',
        rangeSelectorPlotFillGradientColor: 'rgba(123, 104, 238, 0)',
        colorValue: 0.9,
        fillAlpha: 0.4,
        colors: ['#008000', '#ffa500'],
        ylabel: 'Nitter Instances',
      }
    );
    g.ready(function () {
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
async function loadStatsGraph() {
  let graphDiv = document.getElementById('graph-stats');
  try {
    let g = new Dygraph(
      graphDiv,
      "/api/csv/stats",
      {
        title: 'Historic Average Instance Statistics',
        showRangeSelector: true,
        width: '100%',
        rangeSelectorPlotFillColor: 'MediumSlateBlue',
        rangeSelectorPlotFillGradientColor: 'rgba(123, 104, 238, 0)',
        colorValue: 0.9,
        fillAlpha: 0.4,
        colors: ['#008000', '#ffa500', '#6495ED'],
        series: {
          'Tokens AVG': {
            axis: 'y'
          },
          'Limited Tokens AVG': {
            axis: 'y'
          },
          'Requests AVG': {
            axis: 'y2'
          },
        },
        axes: {
          y: {
            axisLabelWidth: 60,
            logscale: "y log scale",
          },
          y2: {
            // set axis-related properties here
            drawGrid: false,
            drawAxis: false,
            logscale: "y log scale",
          },
        },
        ylabel: 'Tokens',
        y2label: 'Requests',
      }
    );
    g.ready(function () {
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