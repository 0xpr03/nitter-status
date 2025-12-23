// SPDX-License-Identifier: AGPL-3.0-only
let chart = undefined;
document.addEventListener("DOMContentLoaded", function() {
    const startDateInput = document.getElementById('startDate');
    const endDateInput = document.getElementById('endDate');
    const submitTime = document.getElementById('submitDateRange');

    // Set initial values in UTC
    const initialEndDate = moment.utc();
    const initialStartDate = moment().subtract(14, 'days').utc();
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
    let jsonData;
    try {
        const response = await fetch('/admin/api/history/'+document.getElementById('graph').getAttribute('data-host'), {
            method: "POST",
                headers: {
                "Content-Type": "application/json",
            },
            body: JSON.stringify({"start": startDate.utc(), "end": endDate.add(1, 'days').utc()})
        });
        jsonData = await response.json();
        
    } catch (error) {
        console.error('Failed to fetch data:', error);
    }
    if (jsonData) {
        createChartOverview(jsonData);
    }
}

function createChartOverview(jsonData) {
    const healthyData = jsonData.health.filter(entry => entry.healthy).map(entry => ({x: moment.unix(entry.time), y: entry.resp_time}));
    const unhealthyData = jsonData.health.filter(entry => !entry.healthy).map(entry => ({x: moment.unix(entry.time), y: entry.resp_time}));
    const statsData = jsonData.stats.map(entry => ({x: moment.unix(entry.time), y: entry.total_requests}));

    const ctx = document.getElementById('graph').getContext('2d');
    if (chart) {
        chart.destroy();
    }
    chart = new Chart(ctx, {
        type: 'line',
        data: {
            datasets: [
                {
                    label: 'Healthy Response Time (ms)',
                    data: healthyData,
                    borderColor: 'green',
                    fill: false,
                },
                {
                    label: 'Unhealthy Response Time (ms)',
                    data: unhealthyData,
                    borderColor: 'red',
                    fill: false,
                },
                {
                    label: 'Total API Requests',
                    data: statsData,
                    borderColor: 'blue',
                    fill: false,
                    yAxisID: 'yStats',
                },
            ]
        },
        options: {
            interaction: {
                mode: 'index',
                intersect: false
            },
            scales: {
                x: {
                    type: 'time',
                    // time: {
                    //     // unit: 'hour'
                    // },
                    scaleLabel: {
                        display: true,
                        labelString: 'Time'
                    }
                },
                y: {
                    scaleLabel: {
                        display: true,
                        labelString: 'Response Time'
                    }
                },
                yStats: {
                    scaleLabel: {
                        display: true,
                        labelString: 'Requests'
                    }
                }
            }
        }
    });

    document.getElementById('graph').onclick = function (evt) {
        const activePoints = chart.getElementsAtEventForMode(evt, 'index', {intersect: true});
        if (activePoints.length > 0) {
            const timestamp = jsonData[activePoints[0]._index].time;
            document.getElementById("error_"+timestamp).scrollIntoView();
        }
    };
}