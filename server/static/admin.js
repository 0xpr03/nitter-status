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
    // Create arrays for y-values for both datasets
    const healthyData = data.global.map(entry => {
        return {x: moment.unix(entry.time), y: entry.alive};
    });

    const unhealthyData = data.global.map(entry => {
        return {x: moment.unix(entry.time), y: entry.dead};
    });

    let statsData = [];
    let statsKeys = [];
    if (data.stats.length > 0) {
        
        statsKeys = Object.keys(data.stats[0]).filter(key => key !== 'time');
        statsData = statsKeys.map(key => ({"key": key, data: data.stats.map(entry => ({x: moment.unix(entry.time), y: entry[key]}))}));
    }
    

    const userUnhealthyData = data.user.filter(entry => entry.dead > 0).map(entry => ({"x": entry.time * 1000, "y": entry.dead}));
    const paramData = {"healthyData":  healthyData, "unhealthyData": unhealthyData, "user": userUnhealthyData, "stats": statsData, "statsKeys": statsKeys};
    
    createChartOverview(paramData);    
}

function mapKey(key) {
    const keyMappings = {
        "limited_accs_max": "Max Lim. Accs",
        "limited_accs_avg": "Avg Lim. Accs",
        "total_accs_max": "Max Accs",
        "total_accs_avg": "Avg Accs",
        "total_requests_max": "Max Requests Req.",
        "total_requests_avg": "Avg Requests Req.",
        "req_photo_rail_max": "Max PhotoRail Req.",
        "req_photo_rail_avg": "Avg PhotoRail Req.",
        "req_user_screen_name_max": "Max UserScreenName Req.",
        "req_user_screen_name_avg": "Avg UserScreenName Req.",
        "req_search_max": "Max Search Req.",
        "req_search_avg": "Avg Search Req.",
        "req_list_tweets_max": "Max ListTweets Req.",
        "req_list_tweets_avg": "Avg ListTweets Req.",
        "req_user_media_max": "Max UserMedia Req.",
        "req_user_media_avg": "Avg UserMedia Req.",
        "req_tweet_detail_max": "Max TweetDetail Req.",
        "req_tweet_detail_avg": "Avg TweetDetail Req.",
        "req_list_max": "Max List Req.",
        "req_list_avg": "Avg List Req.",
        "req_user_tweets_max": "Max UserTweets Req.",
        "req_user_tweets_avg": "Avg UserTweets Req.",
        "req_user_tweets_and_replies_max": "Max UserTweetsAndReplies Req.",
        "req_user_tweets_and_replies_avg": "Avg UserTweetsAndReplies Req.",
    }
    return keyMappings[key];
}

function nameToColor(name) {
    const colors = ["#fafa6e"
    ,"#eafb71"
    ,"#dafb75"
    ,"#cafb7b"
    ,"#b9fa81"
    ,"#a8f989"
    ,"#97f890"
    ,"#85f799"
    ,"#71f5a1"
    ,"#5cf3aa"
    ,"#42f1b3"
    ,"#15efbc"
    ,"#00ecc4"
    ,"#00e9cd"
    ,"#00e6d5"
    ,"#00e3dd"
    ,"#00e0e4"
    ,"#00dcea"
    ,"#00d8f0"
    ,"#00d5f5"
    ,"#00d0f9"
    ,"#00ccfd"
    ,"#00c8ff"
    ,"#00c3ff"
    ,"#00beff"
    ,"#00b9ff"
    ,"#00b4ff"];
    var hash = hashStr(name);
    var index = hash % colors.length;
    return colors[index];
}

function hashStr(str) {
    var hash = 0;
    for (var i = 0; i < str.length; i++) {
        var charCode = str.charCodeAt(i);
        hash += charCode;
    }
    return hash;
}

function createChartOverview(data) {
    const ctx = document.getElementById('graph-health').getContext('2d');
    let high_data = data.stats.length > 1000;
    if (chart) {
        chart.destroy();
    }
    let datasets = [{
        label: 'Healthy',
        data: data.healthyData,
        borderColor: 'green',
        backgroundColor: 'rgba(0, 255, 0, 0.2)',
        fill: true
    },
    {
        label: 'Unhealthy',
        data: data.unhealthyData,
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
    }];
    datasets = datasets.concat(data.stats.map(entry => ({
        yAxisID: 'yStats',
        label: mapKey(entry.key),
        data: entry.data,
        borderColor: nameToColor(entry.key),
        fill: false,
        hidden: (entry.key !== 'total_requests_avg' && entry.key !== 'limited_accs_avg'),
    })));
    console.log(datasets);
    chart = new Chart(ctx, {
        type: 'line',
        data: {
            datasets: datasets,
        },
        options: {
            //responsive: false,
            //maintainAspectRatio: false,
            plugins: {
                colors: {
                    enabled: true
                  },
                decimation: {
                    enabled: high_data,
                    algorithm: 'min-max',
                },
                tooltip: {
                    // callbacks: {
                    //     label: function(context) {
                    //         console.log(context);
                    //         if (context.dataset) {
                    //             if (context.dataset.yAxisID == 'yStats') {
                    //                 let dataset = context.dataset.data[context.dataIndex];
                    //                 if (!dataset) {
                    //                     console.warn(context.dataset);
                    //                     console.warn(context.dataset.data[context.dataIndex]);
                    //                 }
                    //                 let mainValue = dataset.y;
                    //                 let detailValue = dataset.additional.total_requests_max;
                    //                 return [`${context.dataset.label}: ${mainValue}`,`Max Total Requests: ${detailValue}`];
                    //             } else {
                    //                 return `${context.dataset.label}: ${context.formattedValue}`;
                    //             }
                    //         } else {
                    //             console.log(context);
                    //             return null;
                    //         }
                    //         // let label = context.dataset.label || '';
    
                    //         // if (label) {
                    //         //     label += ': ';
                    //         // }
                    //         // if (context.parsed.y !== null) {
                    //         //     label += new Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD' }).format(context.parsed.y);
                    //         // }
                    //         return 'asdf';
                    //     }
                    // }
                }
            },
            animation: !high_data,
            scales: {
                x: {
                    type: 'time',
                    time: {
                        unit: 'day'
                    },
                    autoSkip: true,
                },
                y: {
                    beginAtZero: true
                },
                yStats: {
                    beginAtZero: false
                }
            },
            interaction: {
                mode: 'index',
                intersect: false
            },
        }
    });
}