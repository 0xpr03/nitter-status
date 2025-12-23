// SPDX-License-Identifier: AGPL-3.0-only

// Get the table and checkboxes
const dataTable = document.getElementById('status-tbl');
const toggleColumnCheckboxes = document.querySelectorAll('.toggle-column');

// Add event listeners to individual column toggle checkboxes
toggleColumnCheckboxes.forEach(checkbox => {
    checkbox.addEventListener('change', function () {
        const columnName = checkbox.getAttribute('data-name');
        let checked = checkbox.checked;
        toggleColumn(columnName, checked);
        saveSettings();
    });
});

function saveSettings() {
    let data = {};
    toggleColumnCheckboxes.forEach(checkbox => {
        const columnName = checkbox.getAttribute('data-name');
        let checked = checkbox.checked;
        data[columnName] = checked;
    });
    setCookie(COOKIE_NAME, JSON.stringify(data), 30);
}

// Function to toggle the visibility of a column based on its name or id
function toggleColumn(name, checked) {
    const cellsTd = dataTable.querySelectorAll(`td[data-name="${name}"]`);

    cellsTd.forEach(cell => {
        cell.style.display = checked ? '' : 'none';
    });
    const cells = dataTable.querySelectorAll(`th[data-name="${name}"]`);

    cells.forEach(cell => {
        cell.style.display = checked ? '' : 'none';
    });
}

const COOKIE_NAME = "table_settings";
document.addEventListener("DOMContentLoaded", function (event) {
    let val = getCookie(COOKIE_NAME);
    if (val) {
        let res = JSON.parse(val);
        console.log(res);
        for (const [key, value] of Object.entries(res)) {
            console.log(key, value);
            toggleColumn(key, value);
            toggleColumnCheckboxes.forEach(checkbox => {
                if (checkbox.getAttribute('data-name') == key) {
                    checkbox.checked = value;
                }
            });
        }
    } else {
        toggleColumnCheckboxes.forEach(checkbox => {
            const columnName = checkbox.getAttribute('data-name');
            let checked = checkbox.checked;
            toggleColumn(columnName, checked);
        });
    }
});

function setCookie(name, value, days) {
    var expires = "";
    if (days) {
        var date = new Date();
        date.setTime(date.getTime() + (days * 24 * 60 * 60 * 1000));
        expires = "; expires=" + date.toUTCString();
    }
    document.cookie = name + "=" + (value || "") + expires + "; path=/";
}

function getCookie(name) {
    var nameEQ = name + "=";
    var ca = document.cookie.split(';');
    for (var i = 0; i < ca.length; i++) {
        var c = ca[i];
        while (c.charAt(0) == ' ') c = c.substring(1, c.length);
        if (c.indexOf(nameEQ) == 0) return c.substring(nameEQ.length, c.length);
    }
    return null;
}