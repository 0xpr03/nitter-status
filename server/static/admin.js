document.addEventListener("DOMContentLoaded", function() {
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

let chart = undefined;
async function fetchDataAndCreateChart(startDate,endDate) {
    let data;
    try {
        const response = await fetch('/admin/api/history', {
            method: "POST",
                headers: {
                "Content-Type": "application/json",
            },
            body: JSON.stringify({"start": startDate.utc(), "end": endDate.add(1, 'days').utc()})
        });
        data = await response.json();
    } catch (error) {
        console.error('Failed to fetch data:', error);
    }

    // Extracting data for chart
    const timestamps = data.global.map(entry => entry.time * 1000); // Converting UNIX timestamps to milliseconds
    const healthyData = data.global.map(entry => entry.alive);
    const unhealthyData = data.global.map(entry => entry.dead);
    const userUnhealthyData = data.user.filter(entry => entry.dead > 0).map(entry => ({"x": entry.time * 1000, "y": entry.dead}));
    const paramData = {"global": {"healthyData":  healthyData, "unhealthyData": unhealthyData}, "user": userUnhealthyData};
    console.log(paramData);
    createChart(timestamps, paramData);
    
}

function createChart(timestamps, data) {
    const ctx = document.getElementById('graph').getContext('2d');
    let high_data = timestamps.length > 1000;
    if (chart) {
        chart.destroy();
    }
    chart = new Chart(ctx, {
        type: 'line',
        data: {
            labels: timestamps,
            datasets: [
                {
                    label: 'Healthy',
                    data: data.global.healthyData,
                    borderColor: 'green',
                    backgroundColor: 'rgba(0, 255, 0, 0.2)',
                    fill: true
                },
                {
                    label: 'Unhealthy',
                    data: data.global.unhealthyData,
                    borderColor: 'orange',
                    backgroundColor: 'rgba(241, 90, 34, 0.2)',
                    fill: true
                },
                {
                    label: 'Own Instances Unhealthy',
                    data: data.user,
                    pointRadius: 5,
                    pointBackgroundColor: 'rgba(255, 0, 0, 1)',
                    fill: false,
                    datasetIndex: "x",
                }
            ]
        },
        options: {
            plugins: {
                decimation: {
                    enabled: high_data,
                    algorithm: 'min-max',
                },
            },
            animation: !high_data,
            scales: {
                x: {
                    type: 'time',
                    time: {
                        unit: 'day' // You might want to adjust this based on your data resolution
                    },
                    autoSkip: true,
                },
                y: {
                    beginAtZero: true
                }
            }
        }
    });
}