# \OccupancyApi

All URIs are relative to *https://rest.ably.io*

Method | HTTP request | Description
------------- | ------------- | -------------
[**get_occupancy**](OccupancyApi.md#get_occupancy) | **GET** /chat/v4/rooms/{roomName}/occupancy | Get room occupancy



## get_occupancy

> models::Occupancy get_occupancy(room_name, x_ably_version)
Get room occupancy

Returns current occupancy metrics for a room.

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**room_name** | **String** | The name of the chat room. Sent URL-encoded. | [required] |
**x_ably_version** | Option<**String**> | The Ably API version. The Chat SDK sends `4`. May alternatively be supplied as the `?v=` query parameter.  |  |[default to 4]

### Return type

[**models::Occupancy**](Occupancy.md)

### Authorization

[basicKey](../README.md#basicKey), [bearerToken](../README.md#bearerToken)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)

