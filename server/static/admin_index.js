// SPDX-License-Identifier: AGPL-3.0-only
document.addEventListener("DOMContentLoaded", function (event) {
  const startDateInput = document.getElementById('startDate');
  const endDateInput = document.getElementById('endDate');
  const submitTime = document.getElementById('submitDateRange');

  // Set initial values in UTC
  const initialEndDate = moment.utc();
  const initialStartDate = moment().subtract(30, 'days').utc();
  startDateInput.value = initialStartDate.format('YYYY-MM-DD');
  endDateInput.value = initialEndDate.format('YYYY-MM-DD');;

  submitTime.addEventListener('click', function() {
      const startDate = moment(startDateInput.value, 'YYYY-MM-DD');
      const endDate = moment(endDateInput.value, 'YYYY-MM-DD');

      if (!startDate.isValid() || !endDate.isValid()) {
          alert('Invalid date format. Please use the YYYY-MM-DD format.');
      } else if (startDate.isAfter(endDate)) {
          alert('Invalid date range. Start date must be before the end date.');
      } else {
          fetchDataAndCreateChart(startDate,endDate);
      }
  });
  fetchDataAndCreateChart(initialStartDate,initialEndDate);
});

async function fetchDataAndCreateChart(startDate,endDate) {
  let graphDiv = document.getElementById('graph-health');
  try {
    let g = new Dygraph(
      graphDiv,
      "/admin/api/graphs/overview",
      {
        title: 'Health Overview Instances',
        showRangeSelector: true,
        width: '100%',
        rangeSelectorPlotFillColor: 'MediumSlateBlue',
        rangeSelectorPlotFillGradientColor: 'rgba(123, 104, 238, 0)',
        colorValue: 0.9,
        fillAlpha: 0.4,
        drawPoints: true,
        strokeWidth: 0.0,
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