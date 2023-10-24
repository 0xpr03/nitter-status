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
        createChart(jsonData);
    }
}

function createChart(jsonData) {
    const timeLabels = jsonData.map(dataPoint => new Date(dataPoint.time * 1000));
    console.log(timeLabels);
    const healthyData = jsonData.map(dataPoint => dataPoint.healthy ? (dataPoint.resp_time ? dataPoint.resp_time : 0) : null);
    const unhealthyData = jsonData.map(dataPoint => dataPoint.healthy ? null : (dataPoint.resp_time ? dataPoint.resp_time : 0));

    const ctx = document.getElementById('graph').getContext('2d');
    if (chart) {
        chart.destroy();
    }
    chart = new Chart(ctx, {
        type: 'line',
        data: {
            labels: timeLabels,
            datasets: [
                {
                    label: 'Healthy',
                    data: healthyData,
                    borderColor: 'green',
                    fill: false,
                },
                {
                    label: 'Unhealthy',
                    data: unhealthyData,
                    borderColor: 'red',
                    fill: false,
                },
            ]
        },
        options: {
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